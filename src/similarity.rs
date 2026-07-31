use crate::{BasicBlockInfo, FunctionInfo, InstructionInfo};
use petgraph::Graph;
use rustc_hash::FxHashSet;
use std::collections::HashMap;

pub struct SimilarityAnalyzer;

impl SimilarityAnalyzer {
    /// Calculate Jaccard similarity between two sets of strings
    pub fn jaccard_similarity(set_a: &FxHashSet<String>, set_b: &FxHashSet<String>) -> f64 {
        let intersection = set_a.intersection(set_b).count();
        let union = set_a.union(set_b).count();

        if union == 0 {
            0.0 // Absence of a feature is not positive matching evidence.
        } else {
            intersection as f64 / union as f64
        }
    }

    /// Calculate cosine similarity between two frequency vectors
    pub fn cosine_similarity(
        freq_a: &HashMap<String, usize>,
        freq_b: &HashMap<String, usize>,
    ) -> f64 {
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

    /// Calculate edit distance between two strings.
    /// Uses indexed char vectors and two rolling rows: O(n*m) time, O(m) space
    /// (the old version called chars().nth() in the inner loop — O(n*m*max(n,m))).
    pub fn edit_distance(s1: &str, s2: &str) -> usize {
        let a: Vec<char> = s1.chars().collect();
        let b: Vec<char> = s2.chars().collect();

        let mut prev: Vec<usize> = (0..=b.len()).collect();
        let mut curr = vec![0usize; b.len() + 1];

        for i in 1..=a.len() {
            curr[0] = i;
            for j in 1..=b.len() {
                curr[j] = if a[i - 1] == b[j - 1] {
                    prev[j - 1]
                } else {
                    1 + prev[j].min(curr[j - 1]).min(prev[j - 1])
                };
            }
            std::mem::swap(&mut prev, &mut curr);
        }

        prev[b.len()]
    }

    /// Calculate normalized edit distance (0.0 to 1.0)
    pub fn normalized_edit_distance(s1: &str, s2: &str) -> f64 {
        let max_len = s1.chars().count().max(s2.chars().count());
        if max_len == 0 {
            return 1.0;
        }

        let edit_dist = Self::edit_distance(s1, s2);
        1.0 - (edit_dist as f64 / max_len as f64)
    }

    /// Calculate mnemonic similarity between two basic blocks
    pub fn basic_block_mnemonic_similarity(bb_a: &BasicBlockInfo, bb_b: &BasicBlockInfo) -> f64 {
        let mnemonics_a: FxHashSet<String> = bb_a
            .instructions
            .iter()
            .map(|instr| instr.mnemonic.clone())
            .collect();

        let mnemonics_b: FxHashSet<String> = bb_b
            .instructions
            .iter()
            .map(|instr| instr.mnemonic.clone())
            .collect();

        Self::jaccard_similarity(&mnemonics_a, &mnemonics_b)
    }

    /// Calculate instruction sequence similarity
    pub fn instruction_sequence_similarity(
        instrs_a: &[InstructionInfo],
        instrs_b: &[InstructionInfo],
    ) -> f64 {
        if instrs_a.is_empty() && instrs_b.is_empty() {
            return 1.0;
        }

        if instrs_a.is_empty() || instrs_b.is_empty() {
            return 0.0;
        }

        // Token-level edit distance over mnemonics (one token per instruction,
        // not per character), capped so huge functions don't blow up the
        // O(n*m) DP inside the pairwise fuzzy phase.
        const MAX_TOKENS: usize = 512;
        let seq_a: Vec<&str> = instrs_a
            .iter()
            .take(MAX_TOKENS)
            .map(|i| i.mnemonic.as_str())
            .collect();
        let seq_b: Vec<&str> = instrs_b
            .iter()
            .take(MAX_TOKENS)
            .map(|i| i.mnemonic.as_str())
            .collect();

        let mut prev: Vec<usize> = (0..=seq_b.len()).collect();
        let mut curr = vec![0usize; seq_b.len() + 1];
        for i in 1..=seq_a.len() {
            curr[0] = i;
            for j in 1..=seq_b.len() {
                curr[j] = if seq_a[i - 1] == seq_b[j - 1] {
                    prev[j - 1]
                } else {
                    1 + prev[j].min(curr[j - 1]).min(prev[j - 1])
                };
            }
            std::mem::swap(&mut prev, &mut curr);
        }
        let dist = prev[seq_b.len()];
        let max_len = seq_a.len().max(seq_b.len());
        1.0 - dist as f64 / max_len as f64
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
        // Callee addresses and rendered operands relocate between binaries, so
        // compare call-graph degree here. Matched-neighbor identity is handled
        // by the propagation phase.
        let a = func_a.callees.len();
        let b = func_b.callees.len();
        if a == 0 && b == 0 {
            0.0
        } else {
            1.0 - (a as f64 - b as f64).abs() / a.max(b) as f64
        }
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
                // Only keep small immediates: large values are almost always
                // addresses, which differ between builds and poison the
                // Jaccard score (same cutoff BinDiff/Diaphora use).
                let raw = operand.trim_start_matches('#');
                let value = if let Some(hex) = raw.strip_prefix("0x") {
                    u64::from_str_radix(hex, 16).ok()
                } else {
                    raw.parse::<i64>().ok().map(|v| v.unsigned_abs())
                };
                if let Some(v) = value {
                    if v < 0x10000 {
                        constants.insert(format!("{:#x}", v));
                    }
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
        let constants_a = Self::extract_constants(func_a);
        let constants_b = Self::extract_constants(func_b);
        let strings_a = Self::extract_strings(func_a);
        let strings_b = Self::extract_strings(func_b);
        let mut weights = vec![(Self::control_flow_similarity(func_a, func_b), 0.3)];
        if !func_a.callees.is_empty() || !func_b.callees.is_empty() {
            weights.push((Self::function_call_similarity(func_a, func_b), 0.2));
        }
        if !constants_a.is_empty() || !constants_b.is_empty() {
            weights.push((Self::jaccard_similarity(&constants_a, &constants_b), 0.2));
        }
        if !strings_a.is_empty() || !strings_b.is_empty() {
            weights.push((Self::jaccard_similarity(&strings_a, &strings_b), 0.1));
        }
        if !func_a.instructions.is_empty() || !func_b.instructions.is_empty() {
            weights.push((
                Self::instruction_sequence_similarity(&func_a.instructions, &func_b.instructions),
                0.2,
            ));
        }

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
    pub fn basic_block_similarity_matrix(
        func_a: &FunctionInfo,
        func_b: &FunctionInfo,
    ) -> Vec<Vec<f64>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_set_features_are_not_positive_evidence() {
        let empty = FxHashSet::default();
        assert_eq!(SimilarityAnalyzer::jaccard_similarity(&empty, &empty), 0.0);
    }

    #[test]
    fn unicode_edit_similarity_uses_character_lengths() {
        assert_eq!(SimilarityAnalyzer::normalized_edit_distance("é", "e"), 0.0);
        assert_eq!(SimilarityAnalyzer::normalized_edit_distance("é", "é"), 1.0);
    }
}
