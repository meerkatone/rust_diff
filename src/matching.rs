use crate::algorithms::DiffAlgorithms;
use crate::similarity::SimilarityAnalyzer;
use crate::types::{FunctionInfo, FunctionMatch, MatchDetails, MatchType};
use anyhow::Result;
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;

/// Deterministic tie-breaker: higher similarity wins; then lower index_b
/// (stable for identical scores). Confidence is per-phase so it never
/// differs within a phase.
#[inline]
fn better_candidate(s: f64, idx: usize, bs: f64, bi: usize) -> bool {
    match s.total_cmp(&bs) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => idx < bi,
    }
}

/// Returns true if `name` looks like an auto-generated placeholder
/// (sub_xxxx, FUN_xxxx, loc_xxxx, fcn.xxxx, unnamed, j_sub_...).
/// Matching by such names would collide across unrelated stripped functions.
fn is_auto_generated_name(name: &str) -> bool {
    let n = name.trim_start_matches("j_");
    n.starts_with("sub_")
        || n.starts_with("SUB_")
        || n.starts_with("FUN_")
        || n.starts_with("fun_")
        || n.starts_with("loc_")
        || n.starts_with("fcn.")
        || n.starts_with("func_")
        || n == "unnamed"
        || n.is_empty()
}

pub struct MatchingEngine {
    confidence_threshold: f64,
    similarity_threshold: f64,
}

impl Default for MatchingEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MatchingEngine {
    pub fn new() -> Self {
        Self {
            confidence_threshold: 0.5,
            similarity_threshold: 0.6,
        }
    }

    pub fn with_thresholds(confidence: f64, similarity: f64) -> Self {
        Self {
            confidence_threshold: confidence,
            similarity_threshold: similarity,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn push_match(
        matches: &mut Vec<FunctionMatch>,
        used_a: &mut FxHashSet<usize>,
        used_b: &mut FxHashSet<usize>,
        functions_a: &[FunctionInfo],
        functions_b: &[FunctionInfo],
        idx_a: usize,
        idx_b: usize,
        similarity: f64,
        details: MatchDetails,
        match_type: MatchType,
    ) {
        let confidence = DiffAlgorithms::confidence_for_match(&match_type, similarity);
        matches.push(FunctionMatch {
            function_a: functions_a[idx_a].clone(),
            function_b: functions_b[idx_b].clone(),
            similarity,
            confidence,
            match_type,
            details,
        });
        used_a.insert(idx_a);
        used_b.insert(idx_b);
    }

    /// Primary matching function using multiple heuristics, ordered from
    /// strongest evidence to weakest (BinDiff-style phase ordering).
    pub fn match_functions(
        &self,
        functions_a: &[FunctionInfo],
        functions_b: &[FunctionInfo],
    ) -> Result<Vec<FunctionMatch>> {
        let mut matches = Vec::new();
        let mut used_a = FxHashSet::default();
        let mut used_b = FxHashSet::default();

        // Minimum sizes below which a phase's key carries no signal.
        const MIN_SPP_INSTRUCTIONS: usize = 5;

        // Precompute per-function keys once instead of per-pair.
        let prime_table = DiffAlgorithms::build_mnemonic_prime_table(functions_a, functions_b);
        let md_a: Vec<Option<String>> = functions_a
            .iter()
            .map(|f| Some(DiffAlgorithms::calculate_md_index(f)))
            .collect();
        let md_b: Vec<Option<String>> = functions_b
            .iter()
            .map(|f| Some(DiffAlgorithms::calculate_md_index(f)))
            .collect();
        let spp_key = |f: &FunctionInfo| {
            (f.instructions.len() >= MIN_SPP_INSTRUCTIONS)
                .then(|| DiffAlgorithms::calculate_small_primes_product(f, &prime_table))
        };
        let spp_a: Vec<Option<u64>> = functions_a.iter().map(spp_key).collect();
        let spp_b: Vec<Option<u64>> = functions_b.iter().map(spp_key).collect();

        // 1. Exact matching: WL CFG hash + call-graph degree + instruction
        //    content hash. Structure alone is not enough for "exact" — small
        //    leaf functions with different instructions share WL hashes.
        let exact_key = |f: &FunctionInfo| {
            (!f.instructions.is_empty()).then(|| {
                (
                    DiffAlgorithms::calculate_instruction_content_hash(f),
                    f.cfg_hash.clone(),
                    f.call_graph_hash.clone(),
                )
            })
        };
        let exact_a: Vec<Option<(String, String, String)>> =
            functions_a.iter().map(exact_key).collect();
        let exact_b: Vec<Option<(String, String, String)>> =
            functions_b.iter().map(exact_key).collect();
        self.keyed_matching(
            functions_a,
            functions_b,
            &exact_a,
            &exact_b,
            MatchType::Exact,
            0.0,
            &mut matches,
            &mut used_a,
            &mut used_b,
        );

        // 2. Symbol name matching (near ground truth for real symbols)
        self.name_matching(
            functions_a,
            functions_b,
            &mut matches,
            &mut used_a,
            &mut used_b,
        )?;

        // 3. MD-Index matching (topological fingerprint)
        self.keyed_matching(
            functions_a,
            functions_b,
            &md_a,
            &md_b,
            MatchType::MdIndex,
            self.similarity_threshold,
            &mut matches,
            &mut used_a,
            &mut used_b,
        );

        // 4. Small primes product matching (order-independent instruction multiset)
        self.keyed_matching(
            functions_a,
            functions_b,
            &spp_a,
            &spp_b,
            MatchType::SmallPrimes,
            self.similarity_threshold,
            &mut matches,
            &mut used_a,
            &mut used_b,
        );

        // 5. Structural matching (WL hash equality; instructions may differ
        //    freely). Trivial CFGs share WL hashes vacuously, so require a
        //    minimum number of blocks for the hash to mean anything.
        fn wl_key(f: &FunctionInfo) -> Option<&str> {
            const MIN_STRUCTURAL_BLOCKS: usize = 3;
            (f.basic_blocks.len() >= MIN_STRUCTURAL_BLOCKS).then_some(f.cfg_hash.as_str())
        }
        let wl_a: Vec<Option<&str>> = functions_a.iter().map(wl_key).collect();
        let wl_b: Vec<Option<&str>> = functions_b.iter().map(wl_key).collect();
        self.keyed_matching(
            functions_a,
            functions_b,
            &wl_a,
            &wl_b,
            MatchType::Structural,
            (self.similarity_threshold - 0.2).max(0.3),
            &mut matches,
            &mut used_a,
            &mut used_b,
        );

        // 6. Call-graph match propagation (BinDiff "drill-down"): grow matches
        //    outward from existing anchors through callers/callees.
        self.call_graph_propagation(
            functions_a,
            functions_b,
            &mut matches,
            &mut used_a,
            &mut used_b,
        );

        // 7. Fuzzy matching for whatever survives propagation
        self.fuzzy_matching(
            functions_a,
            functions_b,
            &mut matches,
            &mut used_a,
            &mut used_b,
        )?;

        Ok(matches)
    }

    /// Name-based matching for functions with identical real (non-placeholder)
    /// symbols. A shared real symbol is near ground truth, so the similarity
    /// gate is deliberately loose: similarity indicates how much the function
    /// *changed*, it should not decide whether the match exists.
    fn name_matching(
        &self,
        functions_a: &[FunctionInfo],
        functions_b: &[FunctionInfo],
        matches: &mut Vec<FunctionMatch>,
        used_a: &mut FxHashSet<usize>,
        used_b: &mut FxHashSet<usize>,
    ) -> Result<()> {
        let mut name_map_b: FxHashMap<&str, Vec<usize>> = FxHashMap::default();

        for (i, func_b) in functions_b.iter().enumerate() {
            if !used_b.contains(&i) && !is_auto_generated_name(&func_b.name) {
                name_map_b.entry(func_b.name.as_str()).or_default().push(i);
            }
        }

        for (idx_a, func_a) in functions_a.iter().enumerate() {
            if used_a.contains(&idx_a) || is_auto_generated_name(&func_a.name) {
                continue;
            }
            if let Some(candidates) = name_map_b.get(func_a.name.as_str()) {
                let mut best: Option<(usize, f64, MatchDetails)> = None;
                for &idx in candidates {
                    if !used_b.contains(&idx) {
                        let (similarity, details) =
                            DiffAlgorithms::compute_match_details(func_a, &functions_b[idx]);
                        if best
                            .as_ref()
                            .is_none_or(|(bi, bs, _)| better_candidate(similarity, idx, *bs, *bi))
                        {
                            best = Some((idx, similarity, details));
                        }
                    }
                }
                if let Some((idx, similarity, details)) = best {
                    Self::push_match(
                        matches,
                        used_a,
                        used_b,
                        functions_a,
                        functions_b,
                        idx_a,
                        idx,
                        similarity,
                        details,
                        MatchType::Name,
                    );
                }
            }
        }

        Ok(())
    }

    /// Generic bucketed matching: functions sharing a precomputed key are
    /// candidates; the best by similarity above `min_similarity` wins.
    /// `None` keys mark functions ineligible for the phase (too small for
    /// the key to carry any signal — degenerate buckets would otherwise
    /// pair unrelated leaf functions).
    /// Used for the exact, MD-Index, small-primes-product, and WL-structural phases.
    #[allow(clippy::too_many_arguments)]
    fn keyed_matching<K: std::hash::Hash + Eq>(
        &self,
        functions_a: &[FunctionInfo],
        functions_b: &[FunctionInfo],
        keys_a: &[Option<K>],
        keys_b: &[Option<K>],
        match_type: MatchType,
        min_similarity: f64,
        matches: &mut Vec<FunctionMatch>,
        used_a: &mut FxHashSet<usize>,
        used_b: &mut FxHashSet<usize>,
    ) {
        let mut key_map_b: FxHashMap<&K, Vec<usize>> = FxHashMap::default();
        for (i, key) in keys_b.iter().enumerate() {
            if let (Some(key), false) = (key, used_b.contains(&i)) {
                key_map_b.entry(key).or_default().push(i);
            }
        }

        for (idx_a, func_a) in functions_a.iter().enumerate() {
            if used_a.contains(&idx_a) {
                continue;
            }
            let Some(key_a) = &keys_a[idx_a] else {
                continue;
            };
            if let Some(candidates) = key_map_b.get(key_a) {
                let mut best: Option<(usize, f64, MatchDetails)> = None;
                for &idx in candidates {
                    if !used_b.contains(&idx) {
                        let (similarity, details) =
                            DiffAlgorithms::compute_match_details(func_a, &functions_b[idx]);
                        if similarity >= min_similarity
                            && best.as_ref().is_none_or(|(bi, bs, _)| {
                                better_candidate(similarity, idx, *bs, *bi)
                            })
                        {
                            best = Some((idx, similarity, details));
                        }
                    }
                }
                if let Some((idx, similarity, details)) = best {
                    Self::push_match(
                        matches,
                        used_a,
                        used_b,
                        functions_a,
                        functions_b,
                        idx_a,
                        idx,
                        similarity,
                        details,
                        match_type.clone(),
                    );
                }
            }
        }
    }

    /// BinDiff-style call-graph propagation: if (a, b) are matched, their
    /// unmatched callees (and callers) are compared only against each other
    /// with a relaxed threshold, and accepted matches seed further rounds
    /// until a fixed point. This is what matches heavily-changed stripped
    /// functions that no global heuristic would pair, while keeping the
    /// candidate space tiny.
    fn call_graph_propagation(
        &self,
        functions_a: &[FunctionInfo],
        functions_b: &[FunctionInfo],
        matches: &mut Vec<FunctionMatch>,
        used_a: &mut FxHashSet<usize>,
        used_b: &mut FxHashSet<usize>,
    ) {
        let relaxed_threshold = (self.similarity_threshold - 0.25).max(0.35);

        let addr_to_idx_a: FxHashMap<u64, usize> = functions_a
            .iter()
            .enumerate()
            .map(|(i, f)| (f.address, i))
            .collect();
        let addr_to_idx_b: FxHashMap<u64, usize> = functions_b
            .iter()
            .enumerate()
            .map(|(i, f)| (f.address, i))
            .collect();

        let mut worklist: VecDeque<(usize, usize)> = matches
            .iter()
            .map(|m| {
                (
                    addr_to_idx_a[&m.function_a.address],
                    addr_to_idx_b[&m.function_b.address],
                )
            })
            .collect();

        while let Some((anchor_a, anchor_b)) = worklist.pop_front() {
            // Two neighborhoods: callees of the anchors, then callers.
            let neighborhoods = [
                (
                    &functions_a[anchor_a].callees,
                    &functions_b[anchor_b].callees,
                ),
                (
                    &functions_a[anchor_a].callers,
                    &functions_b[anchor_b].callers,
                ),
            ];

            for (neighbors_a, neighbors_b) in neighborhoods {
                let cand_a: Vec<usize> = neighbors_a
                    .iter()
                    .filter_map(|addr| addr_to_idx_a.get(addr).copied())
                    .filter(|i| !used_a.contains(i))
                    .collect();
                let cand_b: Vec<usize> = neighbors_b
                    .iter()
                    .filter_map(|addr| addr_to_idx_b.get(addr).copied())
                    .filter(|i| !used_b.contains(i))
                    .collect();

                if cand_a.is_empty() || cand_b.is_empty() {
                    continue;
                }

                // Score all pairs in this (small) neighborhood, then accept
                // greedily best-first for deterministic, conflict-free picks.
                let mut scored: Vec<(usize, usize, f64, MatchDetails)> = Vec::new();
                for &ia in &cand_a {
                    for &ib in &cand_b {
                        let (similarity, details) = DiffAlgorithms::compute_match_details(
                            &functions_a[ia],
                            &functions_b[ib],
                        );
                        if similarity >= relaxed_threshold {
                            scored.push((ia, ib, similarity, details));
                        }
                    }
                }
                scored.sort_by(|x, y| {
                    y.2.total_cmp(&x.2)
                        .then_with(|| x.0.cmp(&y.0))
                        .then_with(|| x.1.cmp(&y.1))
                });

                for (ia, ib, similarity, details) in scored {
                    if !used_a.contains(&ia) && !used_b.contains(&ib) {
                        Self::push_match(
                            matches,
                            used_a,
                            used_b,
                            functions_a,
                            functions_b,
                            ia,
                            ib,
                            similarity,
                            details,
                            MatchType::CallGraph,
                        );
                        worklist.push_back((ia, ib));
                    }
                }
            }
        }
    }

    /// Fuzzy matching for remaining functions, blending primary and comprehensive similarity
    fn fuzzy_matching(
        &self,
        functions_a: &[FunctionInfo],
        functions_b: &[FunctionInfo],
        matches: &mut Vec<FunctionMatch>,
        used_a: &mut FxHashSet<usize>,
        used_b: &mut FxHashSet<usize>,
    ) -> Result<()> {
        // Tiny functions carry too little signal for fuzzy matching; they are
        // only matched by exact/name/call-graph evidence. For larger functions,
        // run expensive metrics only against the closest size/shape candidates.
        const MIN_FUZZY_INSTRUCTIONS: usize = 5;
        const MAX_CANDIDATES_PER_FUNCTION: usize = 48;
        const CANDIDATE_WINDOW: usize = 256;
        let mut b_by_size: Vec<(usize, usize)> = functions_b
            .iter()
            .enumerate()
            .filter(|(i, f)| !used_b.contains(i) && f.instructions.len() >= MIN_FUZZY_INSTRUCTIONS)
            .map(|(i, f)| (i, f.instructions.len()))
            .collect();
        b_by_size.sort_by_key(|&(i, count)| (count, i));

        let nested: Vec<Vec<(usize, usize, f64, MatchDetails)>> = functions_a
            .par_iter()
            .enumerate()
            .filter(|(idx_a, f)| {
                !used_a.contains(idx_a) && f.instructions.len() >= MIN_FUZZY_INSTRUCTIONS
            })
            .map(|(idx_a, func_a)| {
                let count_a = func_a.instructions.len();
                let pivot = b_by_size.partition_point(|&(_, count)| count < count_a);
                let start = pivot.saturating_sub(CANDIDATE_WINDOW);
                let end = pivot.saturating_add(CANDIDATE_WINDOW).min(b_by_size.len());
                let mut nearby = b_by_size[start..end].to_vec();
                nearby.sort_by_key(|&(idx_b, count_b)| {
                    (
                        count_a.abs_diff(count_b),
                        func_a
                            .basic_blocks
                            .len()
                            .abs_diff(functions_b[idx_b].basic_blocks.len()),
                        idx_b,
                    )
                });
                nearby.truncate(MAX_CANDIDATES_PER_FUNCTION);

                nearby
                    .into_iter()
                    .filter_map(|(idx_b, _)| {
                        let func_b = &functions_b[idx_b];
                        let (primary, details) =
                            DiffAlgorithms::compute_match_details(func_a, func_b);
                        let comprehensive =
                            SimilarityAnalyzer::comprehensive_similarity(func_a, func_b);
                        let similarity = (primary * 0.6 + comprehensive * 0.4).clamp(0.0, 1.0);
                        let confidence =
                            DiffAlgorithms::confidence_for_match(&MatchType::Heuristic, similarity);
                        (confidence >= self.confidence_threshold
                            && similarity >= self.similarity_threshold)
                            .then_some((idx_a, idx_b, similarity, details))
                    })
                    .collect()
            })
            .collect();
        let mut candidates: Vec<_> = nested.into_iter().flatten().collect();

        // Deterministic global conflict resolution. Keeping every qualifying
        // alternative prevents a function whose first choice was consumed from
        // being stranded even though its second choice is valid.
        candidates.sort_by(|a, b| {
            b.2.total_cmp(&a.2)
                .then_with(|| a.0.cmp(&b.0))
                .then_with(|| a.1.cmp(&b.1))
        });
        for (idx_a, idx_b, similarity, details) in candidates {
            if !used_a.contains(&idx_a) && !used_b.contains(&idx_b) {
                Self::push_match(
                    matches,
                    used_a,
                    used_b,
                    functions_a,
                    functions_b,
                    idx_a,
                    idx_b,
                    similarity,
                    details,
                    MatchType::Heuristic,
                );
            }
        }

        Ok(())
    }
}
