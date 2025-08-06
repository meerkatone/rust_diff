use crate::{DiffResult, FunctionMatch, MatchType};
use std::collections::HashMap;

pub struct DiffUI;

impl DiffUI {
    /// Generate a text-based diff report
    pub fn generate_text_report(diff_result: &DiffResult) -> String {
        let mut report = String::new();
        
        // Header
        report.push_str("=".repeat(60).as_str());
        report.push_str("\n                 BINARY DIFF REPORT\n");
        report.push_str("=".repeat(60).as_str());
        report.push_str("\n\n");
        
        // Summary
        report.push_str("SUMMARY:\n");
        report.push_str(&format!("  Total Matches: {}\n", diff_result.matched_functions.len()));
        report.push_str(&format!("  Unmatched Functions A: {}\n", diff_result.unmatched_functions_a.len()));
        report.push_str(&format!("  Unmatched Functions B: {}\n", diff_result.unmatched_functions_b.len()));
        report.push_str(&format!("  Overall Similarity: {:.4}\n", diff_result.similarity_score));
        report.push_str("\n");
        
        // Match type breakdown
        let match_counts = Self::count_match_types(&diff_result.matched_functions);
        report.push_str("MATCH TYPE BREAKDOWN:\n");
        report.push_str(&format!("  Exact Matches: {}\n", match_counts.get(&MatchType::Exact).unwrap_or(&0)));
        report.push_str(&format!("  Structural Matches: {}\n", match_counts.get(&MatchType::Structural).unwrap_or(&0)));
        report.push_str(&format!("  Heuristic Matches: {}\n", match_counts.get(&MatchType::Heuristic).unwrap_or(&0)));
        report.push_str(&format!("  Manual Matches: {}\n", match_counts.get(&MatchType::Manual).unwrap_or(&0)));
        report.push_str("\n");
        
        // Detailed matches
        report.push_str("DETAILED MATCHES:\n");
        report.push_str("-".repeat(60).as_str());
        report.push_str("\n");
        
        let mut sorted_matches = diff_result.matched_functions.clone();
        sorted_matches.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
        
        for (i, match_result) in sorted_matches.iter().enumerate() {
            report.push_str(&format!("{}. {} <-> {}\n", 
                i + 1,
                match_result.function_a.name,
                match_result.function_b.name
            ));
            report.push_str(&format!("   Addresses: 0x{:x} <-> 0x{:x}\n",
                match_result.function_a.address,
                match_result.function_b.address
            ));
            report.push_str(&format!("   Similarity: {:.4} | Confidence: {:.4} | Type: {:?}\n",
                match_result.similarity,
                match_result.confidence,
                match_result.match_type
            ));
            report.push_str(&format!("   Size: {} bytes <-> {} bytes\n",
                match_result.function_a.size,
                match_result.function_b.size
            ));
            report.push_str(&format!("   Basic Blocks: {} <-> {}\n",
                match_result.function_a.basic_blocks.len(),
                match_result.function_b.basic_blocks.len()
            ));
            report.push_str(&format!("   Instructions: {} <-> {}\n",
                match_result.function_a.instructions.len(),
                match_result.function_b.instructions.len()
            ));
            report.push_str("\n");
        }
        
        // Unmatched functions
        if !diff_result.unmatched_functions_a.is_empty() {
            report.push_str("UNMATCHED FUNCTIONS IN BINARY A:\n");
            report.push_str("-".repeat(60).as_str());
            report.push_str("\n");
            
            for func in &diff_result.unmatched_functions_a {
                report.push_str(&format!("  {} (0x{:x}) - {} bytes, {} BBs\n",
                    func.name,
                    func.address,
                    func.size,
                    func.basic_blocks.len()
                ));
            }
            report.push_str("\n");
        }
        
        if !diff_result.unmatched_functions_b.is_empty() {
            report.push_str("UNMATCHED FUNCTIONS IN BINARY B:\n");
            report.push_str("-".repeat(60).as_str());
            report.push_str("\n");
            
            for func in &diff_result.unmatched_functions_b {
                report.push_str(&format!("  {} (0x{:x}) - {} bytes, {} BBs\n",
                    func.name,
                    func.address,
                    func.size,
                    func.basic_blocks.len()
                ));
            }
            report.push_str("\n");
        }
        
        report
    }

    /// Count matches by type
    fn count_match_types(matches: &[FunctionMatch]) -> HashMap<MatchType, usize> {
        let mut counts = HashMap::new();
        
        for match_result in matches {
            *counts.entry(match_result.match_type.clone()).or_insert(0) += 1;
        }
        
        counts
    }

    /// Generate a color-coded terminal output
    pub fn generate_colored_report(diff_result: &DiffResult) -> String {
        let mut report = String::new();
        
        // ANSI color codes
        let red = "\x1b[31m";
        let green = "\x1b[32m";
        let yellow = "\x1b[33m";
        let blue = "\x1b[34m";
        let magenta = "\x1b[35m";
        let cyan = "\x1b[36m";
        let reset = "\x1b[0m";
        let bold = "\x1b[1m";
        
        // Header
        report.push_str(&format!("{}{}{}", bold, cyan, "=".repeat(60)));
        report.push_str(&format!("\n{}{}                 BINARY DIFF REPORT", bold, cyan));
        report.push_str(&format!("\n{}{}{}", bold, cyan, "=".repeat(60)));
        report.push_str(&format!("{}\n\n", reset));
        
        // Summary with colors
        report.push_str(&format!("{}{}SUMMARY:{}\n", bold, blue, reset));
        report.push_str(&format!("  {}Total Matches:{} {}\n", green, reset, diff_result.matched_functions.len()));
        report.push_str(&format!("  {}Unmatched Functions A:{} {}\n", red, reset, diff_result.unmatched_functions_a.len()));
        report.push_str(&format!("  {}Unmatched Functions B:{} {}\n", red, reset, diff_result.unmatched_functions_b.len()));
        report.push_str(&format!("  {}Overall Similarity:{} {:.4}\n", cyan, reset, diff_result.similarity_score));
        report.push_str("\n");
        
        // Detailed matches with confidence-based coloring
        report.push_str(&format!("{}{}DETAILED MATCHES:{}\n", bold, blue, reset));
        report.push_str(&format!("{}{}", yellow, "-".repeat(60)));
        report.push_str(&format!("{}\n", reset));
        
        let mut sorted_matches = diff_result.matched_functions.clone();
        sorted_matches.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
        
        for (i, match_result) in sorted_matches.iter().enumerate() {
            let confidence_color = if match_result.confidence > 0.8 {
                green
            } else if match_result.confidence > 0.6 {
                yellow
            } else {
                red
            };
            
            let match_type_color = match match_result.match_type {
                MatchType::Exact => green,
                MatchType::Structural => yellow,
                MatchType::Heuristic => magenta,
                MatchType::Manual => cyan,
            };
            
            report.push_str(&format!("{}{}. {}{} <-> {}{}\n", 
                bold, i + 1, green, match_result.function_a.name, match_result.function_b.name, reset
            ));
            report.push_str(&format!("   Addresses: {}0x{:x}{} <-> {}0x{:x}{}\n",
                cyan, match_result.function_a.address, reset,
                cyan, match_result.function_b.address, reset
            ));
            report.push_str(&format!("   Similarity: {}{:.4}{} | Confidence: {}{:.4}{} | Type: {}{:?}{}\n",
                confidence_color, match_result.similarity, reset,
                confidence_color, match_result.confidence, reset,
                match_type_color, match_result.match_type, reset
            ));
            report.push_str("\n");
        }
        
        report
    }

    /// Generate a progress bar for diff operations
    pub fn generate_progress_bar(current: usize, total: usize, width: usize) -> String {
        if total == 0 {
            return "".to_string();
        }
        
        let progress = (current as f64 / total as f64) * width as f64;
        let filled = "█".repeat(progress as usize);
        let empty = "░".repeat(width - progress as usize);
        
        format!("[{}{}] {}/{} ({:.1}%)", filled, empty, current, total, 
                (current as f64 / total as f64) * 100.0)
    }

    /// Generate a simple diff visualization
    pub fn generate_diff_visualization(match_result: &FunctionMatch) -> String {
        let mut viz = String::new();
        
        viz.push_str(&format!("Function Comparison: {} vs {}\n", 
            match_result.function_a.name, match_result.function_b.name));
        viz.push_str("=".repeat(50).as_str());
        viz.push_str("\n");
        
        // Basic block comparison
        viz.push_str("Basic Block Comparison:\n");
        viz.push_str(&format!("  Function A: {} blocks\n", match_result.function_a.basic_blocks.len()));
        viz.push_str(&format!("  Function B: {} blocks\n", match_result.function_b.basic_blocks.len()));
        
        // Create a simple side-by-side view
        let max_blocks = match_result.function_a.basic_blocks.len().max(match_result.function_b.basic_blocks.len());
        
        for i in 0..max_blocks {
            let block_a = match_result.function_a.basic_blocks.get(i);
            let block_b = match_result.function_b.basic_blocks.get(i);
            
            match (block_a, block_b) {
                (Some(a), Some(b)) => {
                    viz.push_str(&format!("  Block {}: 0x{:x} ({} instrs) | 0x{:x} ({} instrs)\n",
                        i, a.address, a.instructions.len(), b.address, b.instructions.len()));
                }
                (Some(a), None) => {
                    viz.push_str(&format!("  Block {}: 0x{:x} ({} instrs) | <missing>\n",
                        i, a.address, a.instructions.len()));
                }
                (None, Some(b)) => {
                    viz.push_str(&format!("  Block {}: <missing> | 0x{:x} ({} instrs)\n",
                        i, b.address, b.instructions.len()));
                }
                (None, None) => break,
            }
        }
        
        viz.push_str("\n");
        
        // Instruction count comparison
        viz.push_str("Instruction Statistics:\n");
        viz.push_str(&format!("  Function A: {} instructions\n", match_result.function_a.instructions.len()));
        viz.push_str(&format!("  Function B: {} instructions\n", match_result.function_b.instructions.len()));
        
        // Cyclomatic complexity
        viz.push_str(&format!("  Complexity A: {}\n", match_result.function_a.cyclomatic_complexity));
        viz.push_str(&format!("  Complexity B: {}\n", match_result.function_b.cyclomatic_complexity));
        
        viz
    }

    /// Generate a summary table for matches
    pub fn generate_summary_table(matches: &[FunctionMatch]) -> String {
        let mut table = String::new();
        
        // Header
        table.push_str("┌─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐\n");
        table.push_str("│                                                    FUNCTION MATCHES                                                    │\n");
        table.push_str("├─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤\n");
        table.push_str("│ Function A                    │ Function B                    │ Similarity │ Confidence │ Type       │ Size A │ Size B │ BB A │ BB B │\n");
        table.push_str("├─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤\n");
        
        // Rows
        for match_result in matches {
            let func_a_name = if match_result.function_a.name.len() > 28 {
                format!("{}...", &match_result.function_a.name[..25])
            } else {
                match_result.function_a.name.clone()
            };
            
            let func_b_name = if match_result.function_b.name.len() > 28 {
                format!("{}...", &match_result.function_b.name[..25])
            } else {
                match_result.function_b.name.clone()
            };
            
            table.push_str(&format!(
                "│ {:<29} │ {:<29} │ {:<10.4} │ {:<10.4} │ {:<10?} │ {:<6} │ {:<6} │ {:<4} │ {:<4} │\n",
                func_a_name,
                func_b_name,
                match_result.similarity,
                match_result.confidence,
                match_result.match_type,
                match_result.function_a.size,
                match_result.function_b.size,
                match_result.function_a.basic_blocks.len(),
                match_result.function_b.basic_blocks.len()
            ));
        }
        
        table.push_str("└─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘\n");
        
        table
    }
}