use crate::types::{FunctionInfo, FunctionMatch, MatchType, MatchDetails};
use crate::algorithms::DiffAlgorithms;
use crate::similarity::SimilarityAnalyzer;
use anyhow::Result;
use std::collections::HashMap;
use rustc_hash::FxHashMap;
use rayon::prelude::*;

pub struct MatchingEngine {
    confidence_threshold: f64,
    similarity_threshold: f64,
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

    /// Primary matching function using multiple heuristics
    pub fn match_functions(
        &self,
        functions_a: &[FunctionInfo],
        functions_b: &[FunctionInfo],
    ) -> Result<Vec<FunctionMatch>> {
        let mut matches = Vec::new();
        let mut used_a = std::collections::HashSet::new();
        let mut used_b = std::collections::HashSet::new();

        // 1. Exact hash matching (highest confidence)
        self.exact_hash_matching(functions_a, functions_b, &mut matches, &mut used_a, &mut used_b)?;

        // 2. Name matching (high confidence)
        self.name_matching(functions_a, functions_b, &mut matches, &mut used_a, &mut used_b)?;

        // 3. MD-Index matching (medium confidence)
        self.md_index_matching(functions_a, functions_b, &mut matches, &mut used_a, &mut used_b)?;

        // 4. Small primes product matching (medium confidence)
        self.small_primes_matching(functions_a, functions_b, &mut matches, &mut used_a, &mut used_b)?;

        // 5. Structural matching (lower confidence)
        self.structural_matching(functions_a, functions_b, &mut matches, &mut used_a, &mut used_b)?;

        // 6. Fuzzy matching (lowest confidence)
        self.fuzzy_matching(functions_a, functions_b, &mut matches, &mut used_a, &mut used_b)?;

        Ok(matches)
    }

    /// Exact hash matching - functions with identical CFG and call graph hashes
    fn exact_hash_matching(
        &self,
        functions_a: &[FunctionInfo],
        functions_b: &[FunctionInfo],
        matches: &mut Vec<FunctionMatch>,
        used_a: &mut std::collections::HashSet<usize>,
        used_b: &mut std::collections::HashSet<usize>,
    ) -> Result<()> {
        let mut hash_map_b: FxHashMap<String, Vec<usize>> = FxHashMap::default();

        for (i, func_b) in functions_b.iter().enumerate() {
            let combined_hash = format!("{}_{}", func_b.cfg_hash, func_b.call_graph_hash);
            hash_map_b.entry(combined_hash).or_default().push(i);
        }

        for (idx_a, func_a) in functions_a.iter().enumerate() {
            if used_a.contains(&idx_a) {
                continue;
            }
            let combined_hash = format!("{}_{}", func_a.cfg_hash, func_a.call_graph_hash);

            if let Some(candidates) = hash_map_b.get(&combined_hash) {
                for &idx in candidates {
                    if !used_b.contains(&idx) {
                        let func_b = &functions_b[idx];
                        let (similarity, details) = DiffAlgorithms::compute_match_details(func_a, func_b);
                        let confidence = DiffAlgorithms::calculate_confidence(func_a, func_b, similarity);

                        matches.push(FunctionMatch {
                            function_a: func_a.clone(),
                            function_b: func_b.clone(),
                            similarity,
                            confidence,
                            match_type: MatchType::Exact,
                            details,
                        });

                        used_a.insert(idx_a);
                        used_b.insert(idx);
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Name-based matching for functions with identical names
    fn name_matching(
        &self,
        functions_a: &[FunctionInfo],
        functions_b: &[FunctionInfo],
        matches: &mut Vec<FunctionMatch>,
        used_a: &mut std::collections::HashSet<usize>,
        used_b: &mut std::collections::HashSet<usize>,
    ) -> Result<()> {
        let mut name_map_b: HashMap<String, Vec<usize>> = HashMap::new();

        for (i, func_b) in functions_b.iter().enumerate() {
            if !used_b.contains(&i) {
                name_map_b.entry(func_b.name.clone()).or_default().push(i);
            }
        }

        for (idx_a, func_a) in functions_a.iter().enumerate() {
            if used_a.contains(&idx_a) {
                continue;
            }
            if let Some(candidates) = name_map_b.get(&func_a.name) {
                for &idx in candidates {
                    if !used_b.contains(&idx) {
                        let func_b = &functions_b[idx];
                        let (similarity, details) = DiffAlgorithms::compute_match_details(func_a, func_b);
                        let confidence = DiffAlgorithms::calculate_confidence(func_a, func_b, similarity);

                        if confidence >= self.confidence_threshold && similarity >= self.similarity_threshold {
                            matches.push(FunctionMatch {
                                function_a: func_a.clone(),
                                function_b: func_b.clone(),
                                similarity,
                                confidence,
                                match_type: MatchType::Structural,
                                details,
                            });

                            used_a.insert(idx_a);
                            used_b.insert(idx);
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// MD-Index based matching (similar to Diaphora)
    fn md_index_matching(
        &self,
        functions_a: &[FunctionInfo],
        functions_b: &[FunctionInfo],
        matches: &mut Vec<FunctionMatch>,
        used_a: &mut std::collections::HashSet<usize>,
        used_b: &mut std::collections::HashSet<usize>,
    ) -> Result<()> {
        let mut md_map_b: HashMap<String, Vec<usize>> = HashMap::new();

        for (i, func_b) in functions_b.iter().enumerate() {
            if !used_b.contains(&i) {
                let md_index = DiffAlgorithms::calculate_md_index(func_b);
                md_map_b.entry(md_index).or_default().push(i);
            }
        }

        for (idx_a, func_a) in functions_a.iter().enumerate() {
            if used_a.contains(&idx_a) {
                continue;
            }
            let md_index_a = DiffAlgorithms::calculate_md_index(func_a);

            if let Some(candidates) = md_map_b.get(&md_index_a) {
                // Pick the best candidate by similarity
                let mut best: Option<(usize, f64, f64, MatchDetails)> = None;
                for &idx in candidates {
                    if !used_b.contains(&idx) {
                        let func_b = &functions_b[idx];
                        let (similarity, details) = DiffAlgorithms::compute_match_details(func_a, func_b);
                        let confidence = DiffAlgorithms::calculate_confidence(func_a, func_b, similarity);

                        if confidence >= self.confidence_threshold && similarity >= self.similarity_threshold {
                            if best.as_ref().map_or(true, |(_, _, bc, _)| confidence > *bc) {
                                best = Some((idx, similarity, confidence, details));
                            }
                        }
                    }
                }
                if let Some((idx, similarity, confidence, details)) = best {
                    matches.push(FunctionMatch {
                        function_a: func_a.clone(),
                        function_b: functions_b[idx].clone(),
                        similarity,
                        confidence,
                        match_type: MatchType::Heuristic,
                        details,
                    });
                    used_a.insert(idx_a);
                    used_b.insert(idx);
                }
            }
        }

        Ok(())
    }

    /// Small primes product matching
    fn small_primes_matching(
        &self,
        functions_a: &[FunctionInfo],
        functions_b: &[FunctionInfo],
        matches: &mut Vec<FunctionMatch>,
        used_a: &mut std::collections::HashSet<usize>,
        used_b: &mut std::collections::HashSet<usize>,
    ) -> Result<()> {
        let mut primes_map_b: HashMap<u64, Vec<usize>> = HashMap::new();

        for (i, func_b) in functions_b.iter().enumerate() {
            if !used_b.contains(&i) {
                let primes_product = DiffAlgorithms::calculate_small_primes_product(func_b);
                primes_map_b.entry(primes_product).or_default().push(i);
            }
        }

        for (idx_a, func_a) in functions_a.iter().enumerate() {
            if used_a.contains(&idx_a) {
                continue;
            }
            let primes_product_a = DiffAlgorithms::calculate_small_primes_product(func_a);

            if let Some(candidates) = primes_map_b.get(&primes_product_a) {
                let mut best: Option<(usize, f64, f64, MatchDetails)> = None;
                for &idx in candidates {
                    if !used_b.contains(&idx) {
                        let func_b = &functions_b[idx];
                        let (similarity, details) = DiffAlgorithms::compute_match_details(func_a, func_b);
                        let confidence = DiffAlgorithms::calculate_confidence(func_a, func_b, similarity);

                        if confidence >= self.confidence_threshold && similarity >= self.similarity_threshold {
                            if best.as_ref().map_or(true, |(_, _, bc, _)| confidence > *bc) {
                                best = Some((idx, similarity, confidence, details));
                            }
                        }
                    }
                }
                if let Some((idx, similarity, confidence, details)) = best {
                    matches.push(FunctionMatch {
                        function_a: func_a.clone(),
                        function_b: functions_b[idx].clone(),
                        similarity,
                        confidence,
                        match_type: MatchType::Heuristic,
                        details,
                    });
                    used_a.insert(idx_a);
                    used_b.insert(idx);
                }
            }
        }

        Ok(())
    }

    /// Structural matching based on CFG isomorphism
    fn structural_matching(
        &self,
        functions_a: &[FunctionInfo],
        functions_b: &[FunctionInfo],
        matches: &mut Vec<FunctionMatch>,
        used_a: &mut std::collections::HashSet<usize>,
        used_b: &mut std::collections::HashSet<usize>,
    ) -> Result<()> {
        for (idx_a, func_a) in functions_a.iter().enumerate() {
            if used_a.contains(&idx_a) {
                continue;
            }
            let mut best_match: Option<(usize, f64, f64, MatchDetails)> = None;

            for (i, func_b) in functions_b.iter().enumerate() {
                if used_b.contains(&i) {
                    continue;
                }

                if DiffAlgorithms::is_isomorphic_subgraph(func_a, func_b) {
                    let (similarity, details) = DiffAlgorithms::compute_match_details(func_a, func_b);
                    let confidence = DiffAlgorithms::calculate_confidence(func_a, func_b, similarity);

                    if confidence >= self.confidence_threshold && similarity >= self.similarity_threshold {
                        if best_match.as_ref().map_or(true, |(_, _, bc, _)| confidence > *bc) {
                            best_match = Some((i, similarity, confidence, details));
                        }
                    }
                }
            }

            if let Some((idx, similarity, confidence, details)) = best_match {
                matches.push(FunctionMatch {
                    function_a: func_a.clone(),
                    function_b: functions_b[idx].clone(),
                    similarity,
                    confidence,
                    match_type: MatchType::Structural,
                    details,
                });
                used_a.insert(idx_a);
                used_b.insert(idx);
            }
        }

        Ok(())
    }

    /// Fuzzy matching for remaining functions, blending primary and comprehensive similarity
    fn fuzzy_matching(
        &self,
        functions_a: &[FunctionInfo],
        functions_b: &[FunctionInfo],
        matches: &mut Vec<FunctionMatch>,
        used_a: &mut std::collections::HashSet<usize>,
        used_b: &mut std::collections::HashSet<usize>,
    ) -> Result<()> {
        let candidates: Vec<_> = functions_a
            .iter()
            .enumerate()
            .filter(|(idx_a, _)| !used_a.contains(idx_a))
            .par_bridge()
            .filter_map(|(idx_a, func_a)| {
                let mut best_match: Option<(usize, f64, f64, MatchDetails)> = None;

                for (i, func_b) in functions_b.iter().enumerate() {
                    if used_b.contains(&i) {
                        continue;
                    }

                    let (primary, details) = DiffAlgorithms::compute_match_details(func_a, func_b);
                    let comprehensive = SimilarityAnalyzer::comprehensive_similarity(func_a, func_b);
                    let similarity = primary * 0.6 + comprehensive * 0.4;
                    let confidence = DiffAlgorithms::calculate_confidence(func_a, func_b, similarity);

                    if confidence >= self.confidence_threshold
                        && similarity >= self.similarity_threshold
                    {
                        if best_match.as_ref().map_or(true, |(_, _, bc, _)| confidence > *bc) {
                            best_match = Some((i, similarity, confidence, details));
                        }
                    }
                }

                best_match.map(|(idx, similarity, confidence, details)| {
                    (idx_a, func_a.clone(), idx, similarity, confidence, details)
                })
            })
            .collect();

        // Resolve conflicts: multiple functions_a may have matched the same functions_b index
        for (idx_a, func_a, idx_b, similarity, confidence, details) in candidates {
            if !used_a.contains(&idx_a) && !used_b.contains(&idx_b) {
                matches.push(FunctionMatch {
                    function_a: func_a,
                    function_b: functions_b[idx_b].clone(),
                    similarity,
                    confidence,
                    match_type: MatchType::Heuristic,
                    details,
                });
                used_a.insert(idx_a);
                used_b.insert(idx_b);
            }
        }

        Ok(())
    }
}
