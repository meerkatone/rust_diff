use std::collections::HashSet;
use std::time::Instant;
use anyhow::{Result, Context};
use log::info;

pub mod types;
pub mod algorithms;
pub mod similarity;
pub mod matching;
pub mod database;
pub mod ui;
pub mod ffi;
pub mod mock;

pub use types::*;
pub use algorithms::*;
pub use similarity::*;

pub struct BinaryDiffEngine {
    pub similarity_threshold: f64,
    pub confidence_threshold: f64,
}

impl BinaryDiffEngine {
    pub fn new() -> Self {
        Self {
            similarity_threshold: 0.6,
            confidence_threshold: 0.5,
        }
    }

    pub fn with_thresholds(similarity: f64, confidence: f64) -> Self {
        Self {
            similarity_threshold: similarity,
            confidence_threshold: confidence,
        }
    }

    pub fn extract_function_info_mock(&self, binary_name: &str) -> Result<Vec<FunctionInfo>> {
        mock::generate_mock_functions(binary_name)
    }

    pub fn perform_diff_mock(&self, binary_a_name: &str, binary_b_name: &str) -> Result<DiffResult> {
        let start_time = Instant::now();

        info!("Starting binary diff analysis");

        let functions_a = self.extract_function_info_mock(binary_a_name)?;
        let functions_b = self.extract_function_info_mock(binary_b_name)?;

        info!(
            "Extracted {} functions from binary A, {} from binary B",
            functions_a.len(),
            functions_b.len()
        );

        let engine = matching::MatchingEngine::with_thresholds(
            self.confidence_threshold,
            self.similarity_threshold,
        );
        let matches = engine.match_functions(&functions_a, &functions_b)?;

        let matched_a: HashSet<u64> = matches.iter().map(|m| m.function_a.address).collect();
        let matched_b: HashSet<u64> = matches.iter().map(|m| m.function_b.address).collect();

        let unmatched_a: Vec<FunctionInfo> = functions_a
            .into_iter()
            .filter(|f| !matched_a.contains(&f.address))
            .collect();

        let unmatched_b: Vec<FunctionInfo> = functions_b
            .into_iter()
            .filter(|f| !matched_b.contains(&f.address))
            .collect();

        let similarity_score = if !matches.is_empty() {
            matches.iter().map(|m| m.similarity).sum::<f64>() / matches.len() as f64
        } else {
            0.0
        };

        let analysis_time = start_time.elapsed().as_secs_f64();

        info!(
            "Diff analysis completed in {:.2}s: {} matches, similarity: {:.3}",
            analysis_time,
            matches.len(),
            similarity_score
        );

        Ok(DiffResult {
            matched_functions: matches,
            unmatched_functions_a: unmatched_a,
            unmatched_functions_b: unmatched_b,
            similarity_score,
            analysis_time,
            binary_a_name: binary_a_name.to_string(),
            binary_b_name: binary_b_name.to_string(),
        })
    }

    pub fn save_results(&self, diff_result: &DiffResult, output_path: &str) -> Result<()> {
        let json_data = serde_json::to_string_pretty(diff_result)
            .context("Failed to serialize diff results")?;

        std::fs::write(output_path, json_data)
            .context("Failed to write results file")?;

        info!("Results saved to {}", output_path);
        Ok(())
    }
}
