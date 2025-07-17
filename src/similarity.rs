use crate::{FunctionInfo, BasicBlockInfo, InstructionInfo};
use std::collections::HashMap;
use rustc_hash::FxHashSet;
use petgraph::Graph;
use petgraph::graph::NodeIndex;

pub struct SimilarityAnalyzer;

impl SimilarityAnalyzer {
    /// Calculate Jaccard similarity between two sets of strings
    pub fn jaccard_similarity(set_a: &FxHashSet<String>, set_b: &FxHashSet<String>) -> f64 {
        let intersection = set_a.intersection(set_b).count();
        let union = set_a.union(set_b).count();
        
        if union == 0 {
            1.0 // Both sets are empty
        } else {
            intersection as f64 / union as f64
        }
    }

    /// Calculate cosine similarity between two frequency vectors
    pub fn cosine_similarity(freq_a: &HashMap<String, usize>, freq_b: &HashMap<String, usize>) -> f64 {
        let mut dot_product = 0.0;
        let mut norm_a = 0.0;
        let mut norm_b = 0.0;
        
        // Calculate dot product and norms
        for (key, &count_a) in freq_a {
            norm_a += (count_a as f64).powi(2);
            if let Some(&count_b) = freq_b.get(key) {
                dot_product += (count_a as f64) * (count_b as f64);
            }
        }
        
        for &count_b in freq_b.values() {
            norm_b += (count_b as f64).powi(2);
        }
        
        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }
        
        dot_product / (norm_a.sqrt() * norm_b.sqrt())
    }

    /// Calculate edit distance between two strings
    pub fn edit_distance(s1: &str, s2: &str) -> usize {
        let len1 = s1.len();
        let len2 = s2.len();
        let mut dp = vec![vec![0; len2 + 1]; len1 + 1];
        
        // Initialize base cases
        for i in 0..=len1 {
            dp[i][0] = i;
        }
        for j in 0..=len2 {
            dp[0][j] = j;
        }
        
        // Fill DP table
        for i in 1..=len1 {
            for j in 1..=len2 {
                if s1.chars().nth(i - 1) == s2.chars().nth(j - 1) {
                    dp[i][j] = dp[i - 1][j - 1];
                } else {
                    dp[i][j] = 1 + dp[i - 1][j].min(dp[i][j - 1]).min(dp[i - 1][j - 1]);
                }
            }
        }
        
        dp[len1][len2]
    }

    /// Calculate normalized edit distance (0.0 to 1.0)
    pub fn normalized_edit_distance(s1: &str, s2: &str) -> f64 {
        let max_len = s1.len().max(s2.len());
        if max_len == 0 {
            return 0.0;
        }
        
        let edit_dist = Self::edit_distance(s1, s2);
        1.0 - (edit_dist as f64 / max_len as f64)
    }

    /// Calculate mnemonic similarity between two basic blocks
    pub fn basic_block_mnemonic_similarity(bb_a: &BasicBlockInfo, bb_b: &BasicBlockInfo) -> f64 {
        let mnemonics_a: FxHashSet<String> = bb_a.instructions.iter()
            .map(|instr| instr.mnemonic.clone())
            .collect();
        
        let mnemonics_b: FxHashSet<String> = bb_b.instructions.iter()
            .map(|instr| instr.mnemonic.clone())
            .collect();
        
        Self::jaccard_similarity(&mnemonics_a, &mnemonics_b)
    }

    /// Calculate instruction sequence similarity
    pub fn instruction_sequence_similarity(instrs_a: &[InstructionInfo], instrs_b: &[InstructionInfo]) -> f64 {
        if instrs_a.is_empty() && instrs_b.is_empty() {
            return 1.0;
        }
        
        if instrs_a.is_empty() || instrs_b.is_empty() {
            return 0.0;
        }
        
        // Create mnemonic sequences
        let seq_a: String = instrs_a.iter()
            .map(|instr| instr.mnemonic.clone())
            .collect::<Vec<_>>()
            .join(" ");
        
        let seq_b: String = instrs_b.iter()
            .map(|instr| instr.mnemonic.clone())
            .collect::<Vec<_>>()
            .join(" ");
        
        Self::normalized_edit_distance(&seq_a, &seq_b)
    }

    /// Calculate control flow similarity using graph comparison
    pub fn control_flow_similarity(func_a: &FunctionInfo, func_b: &FunctionInfo) -> f64 {
        // Create adjacency lists for both functions
        let graph_a = Self::build_cfg_graph(func_a);
        let graph_b = Self::build_cfg_graph(func_b);
        
        // Compare graph structures
        Self::graph_similarity(&graph_a, &graph_b)
    }

    /// Build a control flow graph from function info
    fn build_cfg_graph(func: &FunctionInfo) -> Graph<u64, ()> {
        let mut graph = Graph::new();
        let mut node_map = HashMap::new();
        
        // Add nodes for each basic block
        for bb in &func.basic_blocks {
            let node_idx = graph.add_node(bb.address);
            node_map.insert(bb.address, node_idx);
        }
        
        // Add edges
        for bb in &func.basic_blocks {
            if let Some(&from_idx) = node_map.get(&bb.address) {
                for &target_addr in &bb.edges {
                    if let Some(&to_idx) = node_map.get(&target_addr) {
                        graph.add_edge(from_idx, to_idx, ());
                    }
                }
            }
        }
        
        graph
    }

    /// Calculate similarity between two graphs
    fn graph_similarity(graph_a: &Graph<u64, ()>, graph_b: &Graph<u64, ()>) -> f64 {
        let nodes_a = graph_a.node_count();
        let nodes_b = graph_b.node_count();
        let edges_a = graph_a.edge_count();
        let edges_b = graph_b.edge_count();
        
        if nodes_a == 0 && nodes_b == 0 {
            return 1.0;
        }
        
        // Simple structural similarity
        let node_similarity = if nodes_a == 0 || nodes_b == 0 {
            0.0
        } else {
            1.0 - ((nodes_a as f64 - nodes_b as f64).abs() / nodes_a.max(nodes_b) as f64)
        };
        
        let edge_similarity = if edges_a == 0 && edges_b == 0 {
            1.0
        } else if edges_a == 0 || edges_b == 0 {
            0.0
        } else {
            1.0 - ((edges_a as f64 - edges_b as f64).abs() / edges_a.max(edges_b) as f64)
        };
        
        // Weighted combination
        0.6 * node_similarity + 0.4 * edge_similarity
    }

    /// Calculate function call similarity
    pub fn function_call_similarity(func_a: &FunctionInfo, func_b: &FunctionInfo) -> f64 {
        // Extract function calls from instructions
        let calls_a = Self::extract_function_calls(func_a);
        let calls_b = Self::extract_function_calls(func_b);
        
        Self::jaccard_similarity(&calls_a, &calls_b)
    }

    /// Extract function calls from instructions
    fn extract_function_calls(func: &FunctionInfo) -> FxHashSet<String> {
        let mut calls = FxHashSet::default();
        
        for instr in &func.instructions {
            // Look for call instructions
            if instr.mnemonic.to_lowercase().contains("call") {
                // Extract the target from operands
                if let Some(target) = instr.operands.first() {
                    calls.insert(target.clone());
                }
            }
        }
        
        calls
    }

    /// Calculate constant similarity between functions
    pub fn constant_similarity(func_a: &FunctionInfo, func_b: &FunctionInfo) -> f64 {
        let constants_a = Self::extract_constants(func_a);
        let constants_b = Self::extract_constants(func_b);
        
        Self::jaccard_similarity(&constants_a, &constants_b)
    }

    /// Extract constants from function instructions
    fn extract_constants(func: &FunctionInfo) -> FxHashSet<String> {
        let mut constants = FxHashSet::default();
        
        for instr in &func.instructions {
            for operand in &instr.operands {
                // Look for immediate values (constants)
                if operand.starts_with('#') || operand.starts_with("0x") || operand.parse::<i64>().is_ok() {
                    constants.insert(operand.clone());
                }
            }
        }
        
        constants
    }

    /// Calculate string similarity between functions
    pub fn string_similarity(func_a: &FunctionInfo, func_b: &FunctionInfo) -> f64 {
        let strings_a = Self::extract_strings(func_a);
        let strings_b = Self::extract_strings(func_b);
        
        Self::jaccard_similarity(&strings_a, &strings_b)
    }

    /// Extract string references from function instructions
    fn extract_strings(func: &FunctionInfo) -> FxHashSet<String> {
        let mut strings = FxHashSet::default();
        
        for instr in &func.instructions {
            for operand in &instr.operands {
                // Look for string references (this is a simplified check)
                if operand.starts_with('"') && operand.ends_with('"') {
                    strings.insert(operand.clone());
                }
            }
        }
        
        strings
    }

    /// Calculate overall function similarity using multiple metrics
    pub fn comprehensive_similarity(func_a: &FunctionInfo, func_b: &FunctionInfo) -> f64 {
        let weights = [
            (Self::control_flow_similarity(func_a, func_b), 0.3),
            (Self::function_call_similarity(func_a, func_b), 0.2),
            (Self::constant_similarity(func_a, func_b), 0.2),
            (Self::string_similarity(func_a, func_b), 0.1),
            (Self::instruction_sequence_similarity(&func_a.instructions, &func_b.instructions), 0.2),
        ];
        
        let mut total_weighted_score = 0.0;
        let mut total_weight = 0.0;
        
        for (score, weight) in weights {
            total_weighted_score += score * weight;
            total_weight += weight;
        }
        
        if total_weight > 0.0 {
            total_weighted_score / total_weight
        } else {
            0.0
        }
    }

    /// Calculate basic block similarity matrix
    pub fn basic_block_similarity_matrix(func_a: &FunctionInfo, func_b: &FunctionInfo) -> Vec<Vec<f64>> {
        let mut matrix = Vec::new();
        
        for bb_a in &func_a.basic_blocks {
            let mut row = Vec::new();
            for bb_b in &func_b.basic_blocks {
                let similarity = Self::basic_block_mnemonic_similarity(bb_a, bb_b);
                row.push(similarity);
            }
            matrix.push(row);
        }
        
        matrix
    }
}