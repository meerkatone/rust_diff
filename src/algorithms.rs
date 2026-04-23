use crate::types::{FunctionInfo, MatchDetails};
use crate::similarity::SimilarityAnalyzer;
use std::collections::HashMap;
use rustc_hash::FxHashSet;
use sha2::{Sha256, Digest};

pub struct DiffAlgorithms;

/// Clamp a score to [0.0, 1.0] and replace NaN with 0.0.
#[inline]
fn sanitize_score(x: f64) -> f64 {
    if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) }
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

    /// Calculate Control Flow Graph similarity.
    /// On exact hash match returns 1.0, otherwise falls back to graph-based comparison.
    fn calculate_cfg_similarity(func_a: &FunctionInfo, func_b: &FunctionInfo) -> f64 {
        if func_a.cfg_hash == func_b.cfg_hash {
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

        // Match basic blocks by mnemonic hash
        let mut matched_blocks = 0;
        let mut used_b = FxHashSet::default();

        for bb_a in &func_a.basic_blocks {
            for (i, bb_b) in func_b.basic_blocks.iter().enumerate() {
                if !used_b.contains(&i) && bb_a.mnemonic_hash == bb_b.mnemonic_hash {
                    matched_blocks += 1;
                    used_b.insert(i);
                    break;
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

    /// MD-Index calculation (similar to Diaphora).
    /// Produces a hash from function metadata for fast bucketed matching.
    pub fn calculate_md_index(func: &FunctionInfo) -> String {
        let combined = format!(
            "{}:{}:{}:{}",
            func.size,
            func.basic_blocks.len(),
            func.instructions.len(),
            func.cyclomatic_complexity,
        );

        let mut hasher = Sha256::new();
        hasher.update(combined.as_bytes());
        let result = hasher.finalize();
        hex::encode(&result[..8])
    }

    /// Small primes product calculation
    pub fn calculate_small_primes_product(func: &FunctionInfo) -> u64 {
        let primes = [
            2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79,
            83, 89, 97,
        ];
        let mut product = 1u64;

        for instr in &func.instructions {
            let mnemonic_hash = instr
                .mnemonic
                .bytes()
                .fold(0u64, |acc, b| acc.wrapping_add(b as u64));
            let prime_index = (mnemonic_hash % primes.len() as u64) as usize;
            product = product.wrapping_mul(primes[prime_index]);
        }

        product
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

    /// Calculate confidence score for a match
    pub fn calculate_confidence(
        func_a: &FunctionInfo,
        func_b: &FunctionInfo,
        similarity: f64,
    ) -> f64 {
        let mut confidence = similarity;

        // Boost confidence for exact structural matches
        if func_a.basic_blocks.len() == func_b.basic_blocks.len() {
            confidence += 0.1;
        }

        // Boost confidence for similar complexity
        let complexity_diff =
            (func_a.cyclomatic_complexity as f64 - func_b.cyclomatic_complexity as f64).abs();
        if complexity_diff < 2.0 {
            confidence += 0.1;
        }

        // Boost confidence for similar size
        let max_size = func_a.size.max(func_b.size) as f64;
        if max_size > 0.0 {
            let size_diff = (func_a.size as f64 - func_b.size as f64).abs() / max_size;
            if size_diff < 0.1 {
                confidence += 0.1;
            }
        }

        sanitize_score(confidence)
    }

    /// Perform isomorphic subgraph matching (edge-degree distribution check)
    pub fn is_isomorphic_subgraph(func_a: &FunctionInfo, func_b: &FunctionInfo) -> bool {
        if func_a.basic_blocks.len() != func_b.basic_blocks.len() {
            return false;
        }

        let mut edge_counts_a: HashMap<usize, usize> = HashMap::new();
        let mut edge_counts_b: HashMap<usize, usize> = HashMap::new();

        for bb in &func_a.basic_blocks {
            *edge_counts_a.entry(bb.edges.len()).or_insert(0) += 1;
        }

        for bb in &func_b.basic_blocks {
            *edge_counts_b.entry(bb.edges.len()).or_insert(0) += 1;
        }

        edge_counts_a == edge_counts_b
    }
}
