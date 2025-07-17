use crate::{FunctionInfo, BasicBlockInfo, InstructionInfo, FunctionMatch, MatchType};
use anyhow::Result;
use std::collections::HashMap;
use rustc_hash::FxHashSet;

pub struct DiffAlgorithms;

impl DiffAlgorithms {
    /// Calculate similarity between two functions using multiple metrics
    pub fn calculate_function_similarity(func_a: &FunctionInfo, func_b: &FunctionInfo) -> f64 {
        let mut weighted_score = 0.0;
        
        // Weight distribution similar to BinDiff
        let cfg_weight = 0.5;      // 50% - CFG structure
        let bb_weight = 0.15;      // 15% - Basic blocks
        let instr_weight = 0.10;   // 10% - Instructions
        let edges_weight = 0.25;   // 25% - Edges
        
        // Calculate CFG similarity
        let cfg_similarity = Self::calculate_cfg_similarity(func_a, func_b);
        weighted_score += cfg_similarity * cfg_weight;
        
        // Calculate basic block similarity
        let bb_similarity = Self::calculate_basic_block_similarity(func_a, func_b);
        weighted_score += bb_similarity * bb_weight;
        
        // Calculate instruction similarity
        let instr_similarity = Self::calculate_instruction_similarity(func_a, func_b);
        weighted_score += instr_similarity * instr_weight;
        
        // Calculate edge similarity
        let edge_similarity = Self::calculate_edge_similarity(func_a, func_b);
        weighted_score += edge_similarity * edges_weight;
        
        weighted_score
    }

    /// Calculate Control Flow Graph similarity
    fn calculate_cfg_similarity(func_a: &FunctionInfo, func_b: &FunctionInfo) -> f64 {
        if func_a.cfg_hash == func_b.cfg_hash {
            return 1.0;
        }
        
        // Use graph isomorphism for more detailed analysis
        let bb_count_a = func_a.basic_blocks.len();
        let bb_count_b = func_b.basic_blocks.len();
        
        if bb_count_a == 0 || bb_count_b == 0 {
            return 0.0;
        }
        
        // Simple structural similarity based on basic block count
        let size_diff = ((bb_count_a as f64 - bb_count_b as f64).abs()) / (bb_count_a.max(bb_count_b) as f64);
        1.0 - size_diff
    }

    /// Calculate basic block similarity
    fn calculate_basic_block_similarity(func_a: &FunctionInfo, func_b: &FunctionInfo) -> f64 {
        let bb_count_a = func_a.basic_blocks.len();
        let bb_count_b = func_b.basic_blocks.len();
        
        if bb_count_a == 0 && bb_count_b == 0 {
            return 1.0;
        }
        
        if bb_count_a == 0 || bb_count_b == 0 {
            return 0.0;
        }
        
        // Calculate matched basic blocks using mnemonic hashes
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

    /// Calculate instruction similarity
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
        let mut mnemonic_count_a = HashMap::new();
        let mut mnemonic_count_b = HashMap::new();
        
        for instr in &func_a.instructions {
            *mnemonic_count_a.entry(instr.mnemonic.clone()).or_insert(0) += 1;
        }
        
        for instr in &func_b.instructions {
            *mnemonic_count_b.entry(instr.mnemonic.clone()).or_insert(0) += 1;
        }
        
        // Calculate intersection of mnemonics
        let mut matched_instructions = 0;
        for (mnemonic, count_a) in &mnemonic_count_a {
            if let Some(count_b) = mnemonic_count_b.get(mnemonic) {
                matched_instructions += count_a.min(count_b);
            }
        }
        
        matched_instructions as f64 / instr_count_a.max(instr_count_b) as f64
    }

    /// Calculate edge similarity
    fn calculate_edge_similarity(func_a: &FunctionInfo, func_b: &FunctionInfo) -> f64 {
        let mut total_edges_a = 0;
        let mut total_edges_b = 0;
        
        for bb in &func_a.basic_blocks {
            total_edges_a += bb.edges.len();
        }
        
        for bb in &func_b.basic_blocks {
            total_edges_b += bb.edges.len();
        }
        
        if total_edges_a == 0 && total_edges_b == 0 {
            return 1.0;
        }
        
        if total_edges_a == 0 || total_edges_b == 0 {
            return 0.0;
        }
        
        // Simple edge count similarity
        let edge_diff = ((total_edges_a as f64 - total_edges_b as f64).abs()) / (total_edges_a.max(total_edges_b) as f64);
        1.0 - edge_diff
    }

    /// MD-Index calculation (similar to Diaphora)
    pub fn calculate_md_index(func: &FunctionInfo) -> String {
        let mut md_components = Vec::new();
        
        // Add function size
        md_components.push(func.size.to_string());
        
        // Add basic block count
        md_components.push(func.basic_blocks.len().to_string());
        
        // Add instruction count
        md_components.push(func.instructions.len().to_string());
        
        // Add cyclomatic complexity
        md_components.push(func.cyclomatic_complexity.to_string());
        
        // Create hash from components
        let combined = md_components.join(":");
        format!("{:x}", combined.len() as u64) // Simplified hash
    }

    /// Small primes product calculation
    pub fn calculate_small_primes_product(func: &FunctionInfo) -> u64 {
        let primes = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97];
        let mut product = 1u64;
        
        // Use instruction mnemonics to calculate product
        for instr in &func.instructions {
            let mnemonic_hash = instr.mnemonic.bytes().fold(0u64, |acc, b| acc.wrapping_add(b as u64));
            let prime_index = (mnemonic_hash % primes.len() as u64) as usize;
            product = product.wrapping_mul(primes[prime_index]);
        }
        
        product
    }

    /// Fuzzy hash calculation for functions
    pub fn calculate_fuzzy_hash(func: &FunctionInfo) -> String {
        let mut hash_components = Vec::new();
        
        // Add basic block structure
        for bb in &func.basic_blocks {
            hash_components.push(format!("bb_{:x}_{}", bb.address, bb.instructions.len()));
        }
        
        // Add instruction patterns
        let mut instr_pattern = String::new();
        for instr in &func.instructions {
            instr_pattern.push_str(&instr.mnemonic);
            instr_pattern.push('_');
        }
        hash_components.push(instr_pattern);
        
        // Combine all components
        let combined = hash_components.join(":");
        format!("{:x}", combined.len() as u64)
    }

    /// Calculate confidence score for a match
    pub fn calculate_confidence(func_a: &FunctionInfo, func_b: &FunctionInfo, similarity: f64) -> f64 {
        let mut confidence = similarity;
        
        // Boost confidence for exact structural matches
        if func_a.basic_blocks.len() == func_b.basic_blocks.len() {
            confidence += 0.1;
        }
        
        // Boost confidence for similar complexity
        let complexity_diff = (func_a.cyclomatic_complexity as f64 - func_b.cyclomatic_complexity as f64).abs();
        if complexity_diff < 2.0 {
            confidence += 0.1;
        }
        
        // Boost confidence for similar size
        let size_diff = (func_a.size as f64 - func_b.size as f64).abs() / func_a.size.max(func_b.size) as f64;
        if size_diff < 0.1 {
            confidence += 0.1;
        }
        
        confidence.min(1.0)
    }

    /// Perform isomorphic subgraph matching
    pub fn is_isomorphic_subgraph(func_a: &FunctionInfo, func_b: &FunctionInfo) -> bool {
        // Simplified isomorphism check
        // In practice, this would use more sophisticated graph algorithms
        
        if func_a.basic_blocks.len() != func_b.basic_blocks.len() {
            return false;
        }
        
        // Check if the CFG structures are similar
        let mut edge_counts_a = HashMap::new();
        let mut edge_counts_b = HashMap::new();
        
        for bb in &func_a.basic_blocks {
            *edge_counts_a.entry(bb.edges.len()).or_insert(0) += 1;
        }
        
        for bb in &func_b.basic_blocks {
            *edge_counts_b.entry(bb.edges.len()).or_insert(0) += 1;
        }
        
        edge_counts_a == edge_counts_b
    }
}