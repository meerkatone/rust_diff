use crate::{FunctionInfo, FunctionMatch, MatchType, MatchDetails};
use crate::algorithms::DiffAlgorithms;
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
        let mut used_b = std::collections::HashSet::new();

        // 1. Exact hash matching (highest confidence)
        self.exact_hash_matching(functions_a, functions_b, &mut matches, &mut used_b)?;

        // 2. Name matching (high confidence)
        self.name_matching(functions_a, functions_b, &mut matches, &mut used_b)?;

        // 3. MD-Index matching (medium confidence)
        self.md_index_matching(functions_a, functions_b, &mut matches, &mut used_b)?;

        // 4. Small primes product matching (medium confidence)
        self.small_primes_matching(functions_a, functions_b, &mut matches, &mut used_b)?;

        // 5. Structural matching (lower confidence)
        self.structural_matching(functions_a, functions_b, &mut matches, &mut used_b)?;

        // 6. Fuzzy matching (lowest confidence)
        self.fuzzy_matching(functions_a, functions_b, &mut matches, &mut used_b)?;

        Ok(matches)
    }

    /// Exact hash matching - functions with identical CFG and call graph hashes
    fn exact_hash_matching(
        &self,
        functions_a: &[FunctionInfo],
        functions_b: &[FunctionInfo],
        matches: &mut Vec<FunctionMatch>,
        used_b: &mut std::collections::HashSet<usize>,
    ) -> Result<()> {
        // Create hash maps for efficient lookup
        let mut hash_map_b: FxHashMap<String, Vec<usize>> = FxHashMap::default();
        
        for (i, func_b) in functions_b.iter().enumerate() {
            let combined_hash = format!("{}_{}", func_b.cfg_hash, func_b.call_graph_hash);
            hash_map_b.entry(combined_hash).or_insert_with(Vec::new).push(i);
        }

        for func_a in functions_a {
            let combined_hash = format!("{}_{}", func_a.cfg_hash, func_a.call_graph_hash);
            
            if let Some(candidates) = hash_map_b.get(&combined_hash) {
                for &idx in candidates {
                    if !used_b.contains(&idx) {
                        let func_b = &functions_b[idx];
                        let similarity = DiffAlgorithms::calculate_function_similarity(func_a, func_b);
                        let confidence = DiffAlgorithms::calculate_confidence(func_a, func_b, similarity);
                        
                        matches.push(FunctionMatch {
                            function_a: func_a.clone(),
                            function_b: func_b.clone(),
                            similarity,
                            confidence,
                            match_type: MatchType::Exact,
                            details: MatchDetails {
                                cfg_similarity: 1.0,
                                bb_similarity: 1.0,
                                instruction_similarity: 1.0,
                                edge_similarity: 1.0,
                                name_similarity: 1.0,
                                call_similarity: 1.0,
                            },
                        });
                        
                        used_b.insert(idx);
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Name-based matching for functions with similar names
    fn name_matching(
        &self,
        functions_a: &[FunctionInfo],
        functions_b: &[FunctionInfo],
        matches: &mut Vec<FunctionMatch>,
        used_b: &mut std::collections::HashSet<usize>,
    ) -> Result<()> {
        let mut name_map_b: HashMap<String, Vec<usize>> = HashMap::new();
        
        for (i, func_b) in functions_b.iter().enumerate() {
            if !used_b.contains(&i) {
                name_map_b.entry(func_b.name.clone()).or_insert_with(Vec::new).push(i);
            }
        }

        for func_a in functions_a {
            if let Some(candidates) = name_map_b.get(&func_a.name) {
                for &idx in candidates {
                    if !used_b.contains(&idx) {
                        let func_b = &functions_b[idx];
                        let similarity = DiffAlgorithms::calculate_function_similarity(func_a, func_b);
                        let confidence = DiffAlgorithms::calculate_confidence(func_a, func_b, similarity);
                        
                        if confidence >= self.confidence_threshold && similarity >= self.similarity_threshold {
                            matches.push(FunctionMatch {
                                function_a: func_a.clone(),
                                function_b: func_b.clone(),
                                similarity,
                                confidence,
                                match_type: MatchType::Heuristic,
                                details: MatchDetails {
                                    cfg_similarity: 0.8,
                                    bb_similarity: 0.8,
                                    instruction_similarity: 0.8,
                                    edge_similarity: 0.8,
                                    name_similarity: 0.8,
                                    call_similarity: 0.8,
                                },
                            });
                            
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
        used_b: &mut std::collections::HashSet<usize>,
    ) -> Result<()> {
        let mut md_map_b: HashMap<String, Vec<usize>> = HashMap::new();
        
        for (i, func_b) in functions_b.iter().enumerate() {
            if !used_b.contains(&i) {
                let md_index = DiffAlgorithms::calculate_md_index(func_b);
                md_map_b.entry(md_index).or_insert_with(Vec::new).push(i);
            }
        }

        for func_a in functions_a {
            let md_index_a = DiffAlgorithms::calculate_md_index(func_a);
            
            if let Some(candidates) = md_map_b.get(&md_index_a) {
                for &idx in candidates {
                    if !used_b.contains(&idx) {
                        let func_b = &functions_b[idx];
                        let similarity = DiffAlgorithms::calculate_function_similarity(func_a, func_b);
                        let confidence = DiffAlgorithms::calculate_confidence(func_a, func_b, similarity);
                        
                        if confidence >= self.confidence_threshold && similarity >= self.similarity_threshold {
                            matches.push(FunctionMatch {
                                function_a: func_a.clone(),
                                function_b: func_b.clone(),
                                similarity,
                                confidence,
                                match_type: MatchType::Heuristic,
                                details: MatchDetails {
                                    cfg_similarity: 0.8,
                                    bb_similarity: 0.8,
                                    instruction_similarity: 0.8,
                                    edge_similarity: 0.8,
                                    name_similarity: 0.8,
                                    call_similarity: 0.8,
                                },
                            });
                            
                            used_b.insert(idx);
                            break;
                        }
                    }
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
        used_b: &mut std::collections::HashSet<usize>,
    ) -> Result<()> {
        let mut primes_map_b: HashMap<u64, Vec<usize>> = HashMap::new();
        
        for (i, func_b) in functions_b.iter().enumerate() {
            if !used_b.contains(&i) {
                let primes_product = DiffAlgorithms::calculate_small_primes_product(func_b);
                primes_map_b.entry(primes_product).or_insert_with(Vec::new).push(i);
            }
        }

        for func_a in functions_a {
            let primes_product_a = DiffAlgorithms::calculate_small_primes_product(func_a);
            
            if let Some(candidates) = primes_map_b.get(&primes_product_a) {
                for &idx in candidates {
                    if !used_b.contains(&idx) {
                        let func_b = &functions_b[idx];
                        let similarity = DiffAlgorithms::calculate_function_similarity(func_a, func_b);
                        let confidence = DiffAlgorithms::calculate_confidence(func_a, func_b, similarity);
                        
                        if confidence >= self.confidence_threshold && similarity >= self.similarity_threshold {
                            matches.push(FunctionMatch {
                                function_a: func_a.clone(),
                                function_b: func_b.clone(),
                                similarity,
                                confidence,
                                match_type: MatchType::Heuristic,
                                details: MatchDetails {
                                    cfg_similarity: 0.8,
                                    bb_similarity: 0.8,
                                    instruction_similarity: 0.8,
                                    edge_similarity: 0.8,
                                    name_similarity: 0.8,
                                    call_similarity: 0.8,
                                },
                            });
                            
                            used_b.insert(idx);
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Structural matching based on CFG similarity
    fn structural_matching(
        &self,
        functions_a: &[FunctionInfo],
        functions_b: &[FunctionInfo],
        matches: &mut Vec<FunctionMatch>,
        used_b: &mut std::collections::HashSet<usize>,
    ) -> Result<()> {
        for func_a in functions_a {
            let mut best_match: Option<(usize, f64, f64)> = None;
            
            for (i, func_b) in functions_b.iter().enumerate() {
                if used_b.contains(&i) {
                    continue;
                }
                
                // Check if functions have similar structure
                if DiffAlgorithms::is_isomorphic_subgraph(func_a, func_b) {
                    let similarity = DiffAlgorithms::calculate_function_similarity(func_a, func_b);
                    let confidence = DiffAlgorithms::calculate_confidence(func_a, func_b, similarity);
                    
                    if confidence >= self.confidence_threshold && similarity >= self.similarity_threshold {
                        if let Some((_, _, best_confidence)) = best_match {
                            if confidence > best_confidence {
                                best_match = Some((i, similarity, confidence));
                            }
                        } else {
                            best_match = Some((i, similarity, confidence));
                        }
                    }
                }
            }
            
            if let Some((idx, similarity, confidence)) = best_match {
                let func_b = &functions_b[idx];
                matches.push(FunctionMatch {
                    function_a: func_a.clone(),
                    function_b: func_b.clone(),
                    similarity,
                    confidence,
                    match_type: MatchType::Structural,
                    details: MatchDetails {
                        cfg_similarity: 0.7,
                        bb_similarity: 0.7,
                        instruction_similarity: 0.7,
                        edge_similarity: 0.7,
                        name_similarity: 0.7,
                        call_similarity: 0.7,
                    },
                });
                
                used_b.insert(idx);
            }
        }

        Ok(())
    }

    /// Fuzzy matching for remaining functions
    fn fuzzy_matching(
        &self,
        functions_a: &[FunctionInfo],
        functions_b: &[FunctionInfo],
        matches: &mut Vec<FunctionMatch>,
        used_b: &mut std::collections::HashSet<usize>,
    ) -> Result<()> {
        // Use parallel processing for fuzzy matching
        let candidates: Vec<_> = functions_a.par_iter()
            .filter_map(|func_a| {
                let mut best_match: Option<(usize, f64, f64)> = None;
                
                for (i, func_b) in functions_b.iter().enumerate() {
                    if used_b.contains(&i) {
                        continue;
                    }
                    
                    let similarity = DiffAlgorithms::calculate_function_similarity(func_a, func_b);
                    let confidence = DiffAlgorithms::calculate_confidence(func_a, func_b, similarity);
                    
                    if confidence >= self.confidence_threshold && similarity >= self.similarity_threshold {
                        if let Some((_, _, best_confidence)) = best_match {
                            if confidence > best_confidence {
                                best_match = Some((i, similarity, confidence));
                            }
                        } else {
                            best_match = Some((i, similarity, confidence));
                        }
                    }
                }
                
                best_match.map(|(idx, similarity, confidence)| {
                    (func_a.clone(), idx, similarity, confidence)
                })
            })
            .collect();

        // Add the best matches while avoiding conflicts
        for (func_a, idx, similarity, confidence) in candidates {
            if !used_b.contains(&idx) {
                let func_b = &functions_b[idx];
                matches.push(FunctionMatch {
                    function_a: func_a,
                    function_b: func_b.clone(),
                    similarity,
                    confidence,
                    match_type: MatchType::Heuristic,
                    details: MatchDetails {
                        cfg_similarity: 0.6,
                        bb_similarity: 0.6,
                        instruction_similarity: 0.6,
                        edge_similarity: 0.6,
                        name_similarity: 0.6,
                        call_similarity: 0.6,
                    },
                });
                
                used_b.insert(idx);
            }
        }

        Ok(())
    }

    /// Match a single function against a list of candidates
    pub fn match_single_function(
        &self,
        target_function: &FunctionInfo,
        candidates: &[FunctionInfo],
    ) -> Result<Vec<FunctionMatch>> {
        let mut matches = Vec::new();
        
        for candidate in candidates {
            let similarity = DiffAlgorithms::calculate_function_similarity(target_function, candidate);
            let confidence = DiffAlgorithms::calculate_confidence(target_function, candidate, similarity);
            
            if confidence >= self.confidence_threshold && similarity >= self.similarity_threshold {
                let match_type = if similarity > 0.9 {
                    MatchType::Exact
                } else if confidence > 0.8 {
                    MatchType::Structural
                } else {
                    MatchType::Heuristic
                };
                
                matches.push(FunctionMatch {
                    function_a: target_function.clone(),
                    function_b: candidate.clone(),
                    similarity,
                    confidence,
                    match_type,
                    details: MatchDetails {
                        cfg_similarity: 0.0,
                        bb_similarity: 0.0,
                        instruction_similarity: 0.0,
                        edge_similarity: 0.0,
                        name_similarity: 0.0,
                        call_similarity: 0.0,
                    },
                });
            }
        }
        
        // Sort by confidence (highest first)
        matches.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
        
        Ok(matches)
    }
}