use crate::types::{FunctionInfo, MatchDetails, MatchType};
use crate::similarity::SimilarityAnalyzer;
use std::collections::HashMap;
use rustc_hash::{FxHashMap, FxHashSet, FxHasher};
use std::hash::{Hash, Hasher};
use sha2::{Sha256, Digest};

pub struct DiffAlgorithms;

/// Clamp a score to [0.0, 1.0] and replace NaN with 0.0.
#[inline]
fn sanitize_score(x: f64) -> f64 {
    if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) }
}

/// Deterministic (unseeded) hash of any hashable value.
#[inline]
fn stable_hash<T: Hash>(value: &T) -> u64 {
    let mut hasher = FxHasher::default();
    value.hash(&mut hasher);
    hasher.finish()
}

/// True for call-like mnemonics across common architectures.
fn is_call_mnemonic(m: &str) -> bool {
    let m = m.trim();
    m == "call" || m == "callq" || m == "calll"
        || m == "bl" || m == "blx" || m == "blr"
        || m == "jal" || m == "jalr"
}

/// True for return-like mnemonics across common architectures.
fn is_return_mnemonic(m: &str) -> bool {
    let m = m.trim();
    m.starts_with("ret") || m == "bx lr" || m == "jr ra"
}

impl DiffAlgorithms {
    /// Calculate similarity between two functions using multiple metrics
    /// and return both the weighted score and detailed per-metric breakdown.
    pub fn compute_match_details(func_a: &FunctionInfo, func_b: &FunctionInfo) -> (f64, MatchDetails) {
        let cfg_similarity = sanitize_score(Self::calculate_cfg_similarity(func_a, func_b));
        let bb_similarity = sanitize_score(Self::calculate_basic_block_similarity(func_a, func_b));
        let instruction_similarity = sanitize_score(Self::calculate_instruction_similarity(func_a, func_b));
        let edge_similarity = sanitize_score(Self::calculate_edge_similarity(func_a, func_b));
        let name_similarity = sanitize_score(SimilarityAnalyzer::normalized_edit_distance(&func_a.name, &func_b.name));
        let call_similarity = sanitize_score(SimilarityAnalyzer::function_call_similarity(func_a, func_b));

        let weighted_similarity = sanitize_score(
            cfg_similarity * 0.30
                + call_similarity * 0.20
                + bb_similarity * 0.15
                + instruction_similarity * 0.15
                + name_similarity * 0.10
                + edge_similarity * 0.10,
        );

        let details = MatchDetails {
            cfg_similarity,
            bb_similarity,
            instruction_similarity,
            edge_similarity,
            name_similarity,
            call_similarity,
        };

        (weighted_similarity, details)
    }

    /// Calculate similarity between two functions (returns scalar only).
    pub fn calculate_function_similarity(func_a: &FunctionInfo, func_b: &FunctionInfo) -> f64 {
        let (similarity, _) = Self::compute_match_details(func_a, func_b);
        similarity
    }

    /// Confidence is determined by the algorithm that produced the match
    /// (BinDiff-style), not by ad-hoc boosts. Lower-confidence phases blend
    /// in the computed similarity so weak matches stay distinguishable.
    pub fn confidence_for_match(match_type: &MatchType, similarity: f64) -> f64 {
        let s = sanitize_score(similarity);
        match match_type {
            MatchType::Exact => 1.0,
            MatchType::Name => 0.90 + 0.10 * s,
            MatchType::MdIndex => 0.80 + 0.10 * s,
            MatchType::SmallPrimes => 0.75 + 0.10 * s,
            MatchType::Structural => 0.70 + 0.15 * s,
            MatchType::CallGraph => 0.50 + 0.35 * s,
            MatchType::Heuristic => 0.35 + 0.45 * s,
            MatchType::Manual => 1.0,
        }
    }

    /// Calculate Control Flow Graph similarity.
    /// On exact hash match returns 1.0, otherwise falls back to graph-based comparison.
    fn calculate_cfg_similarity(func_a: &FunctionInfo, func_b: &FunctionInfo) -> f64 {
        if !func_a.cfg_hash.is_empty() && func_a.cfg_hash == func_b.cfg_hash {
            return 1.0;
        }

        // Fall back to graph-based CFG comparison from SimilarityAnalyzer
        SimilarityAnalyzer::control_flow_similarity(func_a, func_b)
    }

    /// Calculate basic block similarity using mnemonic hash matching
    fn calculate_basic_block_similarity(func_a: &FunctionInfo, func_b: &FunctionInfo) -> f64 {
        let bb_count_a = func_a.basic_blocks.len();
        let bb_count_b = func_b.basic_blocks.len();

        if bb_count_a == 0 && bb_count_b == 0 {
            return 1.0;
        }

        if bb_count_a == 0 || bb_count_b == 0 {
            return 0.0;
        }

        // Multiset intersection of mnemonic hashes (O(n) instead of O(n²) scan).
        let mut counts_b: FxHashMap<&str, usize> = FxHashMap::default();
        for bb in &func_b.basic_blocks {
            *counts_b.entry(bb.mnemonic_hash.as_str()).or_insert(0) += 1;
        }

        let mut matched_blocks = 0usize;
        for bb_a in &func_a.basic_blocks {
            if let Some(c) = counts_b.get_mut(bb_a.mnemonic_hash.as_str()) {
                if *c > 0 {
                    *c -= 1;
                    matched_blocks += 1;
                }
            }
        }

        matched_blocks as f64 / bb_count_a.max(bb_count_b) as f64
    }

    /// Calculate instruction similarity using mnemonic histogram matching
    fn calculate_instruction_similarity(func_a: &FunctionInfo, func_b: &FunctionInfo) -> f64 {
        let instr_count_a = func_a.instructions.len();
        let instr_count_b = func_b.instructions.len();

        if instr_count_a == 0 && instr_count_b == 0 {
            return 1.0;
        }

        if instr_count_a == 0 || instr_count_b == 0 {
            return 0.0;
        }

        // Count matching mnemonics (BinDiff style - operands can differ)
        let mut mnemonic_count_a: HashMap<&str, usize> = HashMap::new();
        let mut mnemonic_count_b: HashMap<&str, usize> = HashMap::new();

        for instr in &func_a.instructions {
            *mnemonic_count_a.entry(&instr.mnemonic).or_insert(0) += 1;
        }

        for instr in &func_b.instructions {
            *mnemonic_count_b.entry(&instr.mnemonic).or_insert(0) += 1;
        }

        let mut matched_instructions = 0;
        for (mnemonic, count_a) in &mnemonic_count_a {
            if let Some(count_b) = mnemonic_count_b.get(mnemonic) {
                matched_instructions += count_a.min(count_b);
            }
        }

        matched_instructions as f64 / instr_count_a.max(instr_count_b) as f64
    }

    /// Calculate edge similarity based on edge count difference
    fn calculate_edge_similarity(func_a: &FunctionInfo, func_b: &FunctionInfo) -> f64 {
        let total_edges_a: usize = func_a.basic_blocks.iter().map(|bb| bb.edges.len()).sum();
        let total_edges_b: usize = func_b.basic_blocks.iter().map(|bb| bb.edges.len()).sum();

        if total_edges_a == 0 && total_edges_b == 0 {
            return 1.0;
        }

        if total_edges_a == 0 || total_edges_b == 0 {
            return 0.0;
        }

        let edge_diff = (total_edges_a as f64 - total_edges_b as f64).abs()
            / total_edges_a.max(total_edges_b) as f64;
        1.0 - edge_diff
    }

    /// Weisfeiler-Lehman style labeled CFG hash (Diaphora graph_hashes.py
    /// equivalent). Blocks are labeled by structural category (out-degree
    /// class, contains-call, ends-in-return), then labels are refined for a
    /// few rounds with sorted neighbor labels. The final hash depends only on
    /// graph shape and block categories — not on addresses, block sizes,
    /// registers, or immediates.
    pub fn calculate_wl_cfg_hash(func: &FunctionInfo) -> String {
        let n = func.basic_blocks.len();
        if n == 0 {
            return "empty".to_string();
        }

        let addr_to_idx: FxHashMap<u64, usize> = func
            .basic_blocks
            .iter()
            .enumerate()
            .map(|(i, bb)| (bb.address, i))
            .collect();

        let mut succs: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, bb) in func.basic_blocks.iter().enumerate() {
            for target in &bb.edges {
                if let Some(&j) = addr_to_idx.get(target) {
                    succs[i].push(j);
                    preds[j].push(i);
                }
            }
        }

        // Initial labels: out-degree class (0/1/2/many), has-call, ends-in-return.
        let mut labels: Vec<u64> = func
            .basic_blocks
            .iter()
            .enumerate()
            .map(|(i, bb)| {
                let degree_class = succs[i].len().min(3) as u64;
                let has_call = bb.instructions.iter().any(|ins| is_call_mnemonic(&ins.mnemonic));
                let has_ret = bb
                    .instructions
                    .last()
                    .map_or(false, |ins| is_return_mnemonic(&ins.mnemonic));
                stable_hash(&(degree_class, has_call, has_ret))
            })
            .collect();

        // WL refinement: each round, relabel with own label + sorted neighbor labels.
        for _ in 0..3 {
            let mut next = Vec::with_capacity(n);
            for i in 0..n {
                let mut s: Vec<u64> = succs[i].iter().map(|&j| labels[j]).collect();
                let mut p: Vec<u64> = preds[i].iter().map(|&j| labels[j]).collect();
                s.sort_unstable();
                p.sort_unstable();
                next.push(stable_hash(&(labels[i], s, p)));
            }
            labels = next;
        }

        labels.sort_unstable();
        let mut hasher = Sha256::new();
        for label in &labels {
            hasher.update(label.to_le_bytes());
        }
        hex::encode(&hasher.finalize()[..8])
    }

    /// MD-Index (Dullien/Rolles, as used by BinDiff and Diaphora): a
    /// position-independent topological fingerprint. For each CFG edge,
    /// embed (topological order of source, in/out degrees of both endpoints)
    /// with distinct irrational multipliers and sum 1/sqrt(embedding).
    /// Robust to instruction-level changes; discriminates on structure.
    pub fn calculate_md_index(func: &FunctionInfo) -> String {
        let n = func.basic_blocks.len();
        if n == 0 {
            // Degenerate: address-salted so these never bucket together.
            return format!("md:empty:{:x}", func.address);
        }

        let addr_to_idx: FxHashMap<u64, usize> = func
            .basic_blocks
            .iter()
            .enumerate()
            .map(|(i, bb)| (bb.address, i))
            .collect();

        let mut succs: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut in_deg = vec![0usize; n];
        for (i, bb) in func.basic_blocks.iter().enumerate() {
            // Deterministic successor order regardless of extraction order.
            let mut targets: Vec<u64> = bb.edges.clone();
            targets.sort_unstable();
            for target in targets {
                if let Some(&j) = addr_to_idx.get(&target) {
                    succs[i].push(j);
                    in_deg[j] += 1;
                }
            }
        }

        // Reverse postorder from the entry block as the topological order
        // (back-edges are simply ranked by traversal position).
        let mut order = vec![usize::MAX; n];
        let mut postorder = Vec::with_capacity(n);
        let mut visited = vec![false; n];
        // Iterative DFS; start at entry (block 0), then pick up unreachable blocks.
        for start in 0..n {
            if visited[start] {
                continue;
            }
            let mut stack = vec![(start, 0usize)];
            visited[start] = true;
            while let Some(&mut (node, ref mut child)) = stack.last_mut() {
                if *child < succs[node].len() {
                    let next = succs[node][*child];
                    *child += 1;
                    if !visited[next] {
                        visited[next] = true;
                        stack.push((next, 0));
                    }
                } else {
                    postorder.push(node);
                    stack.pop();
                }
            }
        }
        for (rank, &node) in postorder.iter().rev().enumerate() {
            order[node] = rank;
        }

        let sqrt2 = 2.0_f64.sqrt();
        let sqrt3 = 3.0_f64.sqrt();
        let sqrt5 = 5.0_f64.sqrt();
        let sqrt7 = 7.0_f64.sqrt();
        let sqrt11 = 11.0_f64.sqrt();

        let mut md_index = 0.0_f64;
        for (src, targets) in succs.iter().enumerate() {
            for &dst in targets {
                let embedding = sqrt2 * (order[src] + 1) as f64
                    + sqrt3 * succs[src].len() as f64
                    + sqrt5 * in_deg[src] as f64
                    + sqrt7 * succs[dst].len() as f64
                    + sqrt11 * in_deg[dst] as f64;
                if embedding > 0.0 {
                    md_index += 1.0 / embedding.sqrt();
                }
            }
        }

        if md_index == 0.0 {
            // No CFG edges (single-block function): the topological
            // fingerprint carries no information, so salt with the address
            // to keep unrelated leaf functions out of one giant bucket.
            return format!("md:leaf:{:x}", func.address);
        }

        // Fixed precision so float noise can't split buckets.
        format!("md:{:.8}", md_index)
    }

    /// Build a stable mnemonic -> prime table shared by both binaries:
    /// sorted unique mnemonics are assigned consecutive primes, so the same
    /// mnemonic always maps to the same prime on both sides of the diff.
    pub fn build_mnemonic_prime_table(
        functions_a: &[FunctionInfo],
        functions_b: &[FunctionInfo],
    ) -> FxHashMap<String, u64> {
        let mut mnemonics: FxHashSet<&str> = FxHashSet::default();
        for func in functions_a.iter().chain(functions_b.iter()) {
            for instr in &func.instructions {
                mnemonics.insert(instr.mnemonic.as_str());
            }
        }
        let mut sorted: Vec<&str> = mnemonics.into_iter().collect();
        sorted.sort_unstable();

        let primes = Self::first_n_primes(sorted.len());
        sorted
            .into_iter()
            .zip(primes)
            .map(|(m, p)| (m.to_string(), p))
            .collect()
    }

    fn first_n_primes(n: usize) -> Vec<u64> {
        let mut primes: Vec<u64> = Vec::with_capacity(n);
        let mut candidate = 2u64;
        while primes.len() < n {
            if primes.iter().take_while(|&&p| p * p <= candidate).all(|&p| candidate % p != 0) {
                primes.push(candidate);
            }
            candidate += 1;
        }
        primes
    }

    /// Small primes product (Diaphora SPP): product of one fixed prime per
    /// instruction mnemonic, computed modulo a Mersenne prime so the
    /// order-independence property survives arbitrarily large functions
    /// (the old u64 wrapping_mul silently destroyed it via overflow).
    pub fn calculate_small_primes_product(
        func: &FunctionInfo,
        prime_table: &FxHashMap<String, u64>,
    ) -> u64 {
        const MODULUS: u128 = (1u128 << 61) - 1;
        let mut product = 1u128;

        for instr in &func.instructions {
            let prime = prime_table.get(&instr.mnemonic).copied().unwrap_or(1);
            product = (product * prime as u128) % MODULUS;
        }

        product as u64
    }

    /// Fuzzy hash calculation for functions.
    /// Encodes basic block structure and instruction mnemonic patterns.
    pub fn calculate_fuzzy_hash(func: &FunctionInfo) -> String {
        let mut hash_input = String::new();

        // Encode basic block structure (instruction count per block, not addresses)
        for bb in &func.basic_blocks {
            hash_input.push_str(&format!("bb{}e{}_", bb.instructions.len(), bb.edges.len()));
        }

        // Encode instruction mnemonic sequence
        for instr in &func.instructions {
            hash_input.push_str(&instr.mnemonic);
            hash_input.push('_');
        }

        let mut hasher = Sha256::new();
        hasher.update(hash_input.as_bytes());
        let result = hasher.finalize();
        hex::encode(&result[..8])
    }

    /// Structural equivalence test: WL-refined label multisets must agree.
    /// Far stronger than the old out-degree-histogram check (which accepted
    /// many non-isomorphic graphs), though still not an exact isomorphism test.
    pub fn is_isomorphic_subgraph(func_a: &FunctionInfo, func_b: &FunctionInfo) -> bool {
        if func_a.basic_blocks.len() != func_b.basic_blocks.len() {
            return false;
        }
        Self::calculate_wl_cfg_hash(func_a) == Self::calculate_wl_cfg_hash(func_b)
    }
}
