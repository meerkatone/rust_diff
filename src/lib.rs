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
pub mod il;
pub mod block;

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

    /// Normalize extracted functions: structure-only hashes are computed here
    /// (not in the extractor) so they are deterministic and consistent across
    /// frontends. cfg_hash = WL labeled-graph hash, call_graph_hash = local
    /// call-graph degree signature.
    pub fn preprocess_functions(functions: &mut [FunctionInfo]) {
        use sha2::{Digest, Sha256};
        for func in functions.iter_mut() {
            func.cfg_hash = DiffAlgorithms::calculate_wl_cfg_hash(func);
            func.call_graph_hash = format!("cg:{}:{}", func.callees.len(), func.callers.len());
            func.instruction_count = func.instructions.len();
            func.call_count = func.callees.len();
            for bb in func.basic_blocks.iter_mut() {
                bb.instruction_count = bb.instructions.len();
                if bb.mnemonic_hash.is_empty() {
                    let mut hasher = Sha256::new();
                    for ins in &bb.instructions {
                        hasher.update(ins.mnemonic.as_bytes());
                        hasher.update(b" ");
                    }
                    bb.mnemonic_hash = hex::encode(&hasher.finalize()[..8]);
                }
            }
        }
    }

    /// Strip bulky per-instruction data from result payloads; counts are kept
    /// in instruction_count fields so the UI columns stay correct.
    fn slim_function(func: &mut FunctionInfo) {
        func.instructions.clear();
        for bb in func.basic_blocks.iter_mut() {
            bb.instructions.clear();
        }
    }

    /// Diff two binaries from JSON-encoded `Vec<FunctionInfo>` (the FFI path
    /// used by the Binary Ninja Python frontend).
    pub fn perform_diff_json(&self, json_a: &str, json_b: &str) -> Result<DiffResult> {
        let mut functions_a: Vec<FunctionInfo> =
            serde_json::from_str(json_a).context("Failed to parse functions for binary A")?;
        let mut functions_b: Vec<FunctionInfo> =
            serde_json::from_str(json_b).context("Failed to parse functions for binary B")?;

        Self::preprocess_functions(&mut functions_a);
        Self::preprocess_functions(&mut functions_b);

        let mut result = self.diff_functions(functions_a, functions_b, "binary_a", "binary_b")?;

        for m in result.matched_functions.iter_mut() {
            Self::slim_function(&mut m.function_a);
            Self::slim_function(&mut m.function_b);
        }
        for f in result.unmatched_functions_a.iter_mut() {
            Self::slim_function(f);
        }
        for f in result.unmatched_functions_b.iter_mut() {
            Self::slim_function(f);
        }

        Ok(result)
    }

    fn diff_functions(
        &self,
        functions_a: Vec<FunctionInfo>,
        functions_b: Vec<FunctionInfo>,
        binary_a_name: &str,
        binary_b_name: &str,
    ) -> Result<DiffResult> {
        let start_time = Instant::now();

        info!(
            "Diffing {} functions against {}",
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
