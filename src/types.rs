use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstructionInfo {
    pub address: u64,
    pub mnemonic: String,
    pub operands: Vec<String>,
    pub bytes: Vec<u8>,
    pub length: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicBlockInfo {
    pub address: u64,
    pub size: u64,
    pub instructions: Vec<InstructionInfo>,
    pub edges: Vec<u64>,
    pub mnemonic_hash: String,
    pub instruction_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInfo {
    pub name: String,
    pub address: u64,
    pub size: u64,
    pub basic_blocks: Vec<BasicBlockInfo>,
    pub instructions: Vec<InstructionInfo>,
    pub cyclomatic_complexity: u32,
    pub call_graph_hash: String,
    pub cfg_hash: String,
    pub instruction_count: usize,
    pub call_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffResult {
    pub matched_functions: Vec<FunctionMatch>,
    pub unmatched_functions_a: Vec<FunctionInfo>,
    pub unmatched_functions_b: Vec<FunctionInfo>,
    pub similarity_score: f64,
    pub analysis_time: f64,
    pub binary_a_name: String,
    pub binary_b_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionMatch {
    pub function_a: FunctionInfo,
    pub function_b: FunctionInfo,
    pub similarity: f64,
    pub confidence: f64,
    pub match_type: MatchType,
    pub details: MatchDetails,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchType {
    Exact,
    Structural,
    Heuristic,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchDetails {
    pub cfg_similarity: f64,
    pub bb_similarity: f64,
    pub instruction_similarity: f64,
    pub edge_similarity: f64,
    pub name_similarity: f64,
    pub call_similarity: f64,
}
