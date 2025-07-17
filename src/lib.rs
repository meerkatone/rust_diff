use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::Arc;
use std::time::Instant;
use std::hash::{Hash, Hasher};
use anyhow::{Result, Context};
use log::{info, error, debug, warn};
use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use parking_lot::RwLock;

pub mod algorithms;
pub mod similarity;
pub mod database;
pub mod ui;
pub mod matching;

pub use algorithms::*;
pub use similarity::*;

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

pub struct BinaryDiffEngine {
    pub similarity_threshold: f64,
    pub confidence_threshold: f64,
    pub functions_a: Arc<RwLock<Vec<FunctionInfo>>>,
    pub functions_b: Arc<RwLock<Vec<FunctionInfo>>>,
    pub matches: Arc<RwLock<Vec<FunctionMatch>>>,
}

impl BinaryDiffEngine {
    pub fn new() -> Self {
        Self {
            similarity_threshold: 0.6,
            confidence_threshold: 0.5,
            functions_a: Arc::new(RwLock::new(Vec::new())),
            functions_b: Arc::new(RwLock::new(Vec::new())),
            matches: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn with_thresholds(similarity: f64, confidence: f64) -> Self {
        Self {
            similarity_threshold: similarity,
            confidence_threshold: confidence,
            functions_a: Arc::new(RwLock::new(Vec::new())),
            functions_b: Arc::new(RwLock::new(Vec::new())),
            matches: Arc::new(RwLock::new(Vec::new())),
        }
    }

    // Extract function information from Binary Ninja (mock implementation)
    // In a real implementation, this would use Binary Ninja's Rust API
    pub fn extract_function_info_mock(&self, binary_name: &str) -> Result<Vec<FunctionInfo>> {
        info!("Extracting function information from binary: {}", binary_name);
        
        // Create more realistic mock functions based on common patterns
        let mut functions = Vec::new();
        
        // Common function patterns
        let function_patterns = vec![
            ("main", 0x1000, 200, 3, 5),
            ("printf", 0x1200, 50, 1, 1),
            ("malloc", 0x1300, 80, 2, 2),
            ("strcmp", 0x1400, 120, 4, 3),
            ("memcpy", 0x1500, 90, 2, 2),
            ("init_function", 0x1600, 150, 3, 4),
            ("cleanup_function", 0x1700, 100, 2, 3),
            ("process_data", 0x1800, 300, 6, 8),
        ];
        
        for (i, (name, base_addr, size, bb_count, complexity)) in function_patterns.iter().enumerate() {
            let mut basic_blocks = Vec::new();
            let mut all_instructions = Vec::new();
            
            // Create realistic basic blocks
            for bb_idx in 0..*bb_count {
                let bb_addr = base_addr + (bb_idx * 40) as u64;
                let mut instructions = Vec::new();
                
                // Create instructions for this basic block
                for instr_idx in 0..3 {
                    let instr_addr = bb_addr + (instr_idx * 4) as u64;
                    let instruction = InstructionInfo {
                        address: instr_addr,
                        mnemonic: match instr_idx {
                            0 => "push".to_string(),
                            1 => "mov".to_string(),
                            _ => "call".to_string(),
                        },
                        operands: match instr_idx {
                            0 => vec!["rbp".to_string()],
                            1 => vec!["rsp".to_string(), "rbp".to_string()],
                            _ => vec![format!("func_{}", i)],
                        },
                        bytes: vec![0x55, 0x48, 0x89, 0xe5],
                        length: 4,
                    };
                    instructions.push(instruction.clone());
                    all_instructions.push(instruction);
                }
                
                // Create basic block with edges
                let mut edges = Vec::new();
                if bb_idx < bb_count - 1 {
                    edges.push(base_addr + ((bb_idx + 1) * 40) as u64);
                }
                
                let basic_block = BasicBlockInfo {
                    address: bb_addr,
                    size: 40,
                    instructions,
                    edges,
                    mnemonic_hash: format!("bb_hash_{}_{}", i, bb_idx),
                    instruction_count: 3,
                };
                basic_blocks.push(basic_block);
            }
            
            // Create function hash based on structure
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(&format!("{}_{}", name, bb_count), &mut hasher);
            let cfg_hash = format!("cfg_{:x}", std::hash::Hasher::finish(&hasher));
            
            let mut call_hasher = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(&format!("{}_{}", name, complexity), &mut call_hasher);
            let call_graph_hash = format!("call_{:x}", std::hash::Hasher::finish(&call_hasher));
            
            let function = FunctionInfo {
                name: name.to_string(),
                address: *base_addr,
                size: *size,
                basic_blocks,
                instructions: all_instructions,
                cyclomatic_complexity: *complexity as u32,
                call_graph_hash,
                cfg_hash,
                instruction_count: (bb_count * 3) as usize,
                call_count: if *complexity > 2 { 2 } else { 1 },
            };
            functions.push(function);
        }
        
        info!("Extracted {} functions", functions.len());
        Ok(functions)
    }

    pub fn compare_functions(&self, functions_a: &[FunctionInfo], functions_b: &[FunctionInfo]) -> Result<Vec<FunctionMatch>> {
        info!("Starting function comparison");
        let mut matches = Vec::new();
        let mut used_b = HashSet::new();
        
        // 1. Exact hash matching
        let exact_matches = self.exact_hash_matching(functions_a, functions_b, &mut used_b)?;
        matches.extend(exact_matches);
        info!("Found {} exact matches", matches.len());
        
        // 2. Name matching
        let name_matches = self.name_matching(functions_a, functions_b, &mut used_b)?;
        matches.extend(name_matches);
        info!("Found {} name matches", matches.len());
        
        // 3. Structural matching
        let structural_matches = self.structural_matching(functions_a, functions_b, &mut used_b)?;
        matches.extend(structural_matches);
        info!("Found {} structural matches", matches.len());
        
        // 4. Heuristic matching
        let heuristic_matches = self.heuristic_matching(functions_a, functions_b, &mut used_b)?;
        matches.extend(heuristic_matches);
        info!("Found {} total matches", matches.len());
        
        Ok(matches)
    }

    fn exact_hash_matching(&self, functions_a: &[FunctionInfo], functions_b: &[FunctionInfo], used_b: &mut HashSet<usize>) -> Result<Vec<FunctionMatch>> {
        let mut matches = Vec::new();
        
        // Create hash map for efficient lookup
        let mut hash_map: FxHashMap<String, Vec<usize>> = FxHashMap::default();
        
        for (i, func_b) in functions_b.iter().enumerate() {
            let combined_hash = format!("{}_{}", func_b.cfg_hash, func_b.call_graph_hash);
            hash_map.entry(combined_hash).or_insert_with(Vec::new).push(i);
        }
        
        for func_a in functions_a {
            let combined_hash = format!("{}_{}", func_a.cfg_hash, func_a.call_graph_hash);
            
            if let Some(candidates) = hash_map.get(&combined_hash) {
                for &idx in candidates {
                    if !used_b.contains(&idx) {
                        let func_b = &functions_b[idx];
                        let (similarity, details) = self.calculate_detailed_similarity(func_a, func_b);
                        let confidence = self.calculate_confidence(func_a, func_b, similarity);
                        
                        matches.push(FunctionMatch {
                            function_a: func_a.clone(),
                            function_b: func_b.clone(),
                            similarity,
                            confidence,
                            match_type: MatchType::Exact,
                            details,
                        });
                        
                        used_b.insert(idx);
                        break;
                    }
                }
            }
        }
        
        Ok(matches)
    }

    fn name_matching(&self, functions_a: &[FunctionInfo], functions_b: &[FunctionInfo], used_b: &mut HashSet<usize>) -> Result<Vec<FunctionMatch>> {
        let mut matches = Vec::new();
        
        // Create name map for efficient lookup
        let mut name_map: HashMap<String, Vec<usize>> = HashMap::new();
        
        for (i, func_b) in functions_b.iter().enumerate() {
            if !used_b.contains(&i) {
                name_map.entry(func_b.name.clone()).or_insert_with(Vec::new).push(i);
            }
        }
        
        for func_a in functions_a {
            if let Some(candidates) = name_map.get(&func_a.name) {
                for &idx in candidates {
                    if !used_b.contains(&idx) {
                        let func_b = &functions_b[idx];
                        let (similarity, details) = self.calculate_detailed_similarity(func_a, func_b);
                        let confidence = self.calculate_confidence(func_a, func_b, similarity);
                        
                        if confidence >= self.confidence_threshold && similarity >= self.similarity_threshold {
                            matches.push(FunctionMatch {
                                function_a: func_a.clone(),
                                function_b: func_b.clone(),
                                similarity,
                                confidence,
                                match_type: MatchType::Heuristic,
                                details,
                            });
                            
                            used_b.insert(idx);
                            break;
                        }
                    }
                }
            }
        }
        
        Ok(matches)
    }

    fn structural_matching(&self, functions_a: &[FunctionInfo], functions_b: &[FunctionInfo], used_b: &mut HashSet<usize>) -> Result<Vec<FunctionMatch>> {
        let mut matches = Vec::new();
        
        for func_a in functions_a {
            let mut best_match: Option<(usize, f64, f64, MatchDetails)> = None;
            
            for (i, func_b) in functions_b.iter().enumerate() {
                if used_b.contains(&i) {
                    continue;
                }
                
                // Check structural similarity
                if self.is_structurally_similar(func_a, func_b) {
                    let (similarity, details) = self.calculate_detailed_similarity(func_a, func_b);
                    let confidence = self.calculate_confidence(func_a, func_b, similarity);
                    
                    if confidence >= self.confidence_threshold && similarity >= self.similarity_threshold {
                        if let Some((_, _, best_confidence, _)) = best_match {
                            if confidence > best_confidence {
                                best_match = Some((i, similarity, confidence, details));
                            }
                        } else {
                            best_match = Some((i, similarity, confidence, details));
                        }
                    }
                }
            }
            
            if let Some((idx, similarity, confidence, details)) = best_match {
                let func_b = &functions_b[idx];
                matches.push(FunctionMatch {
                    function_a: func_a.clone(),
                    function_b: func_b.clone(),
                    similarity,
                    confidence,
                    match_type: MatchType::Structural,
                    details,
                });
                
                used_b.insert(idx);
            }
        }
        
        Ok(matches)
    }

    fn heuristic_matching(&self, functions_a: &[FunctionInfo], functions_b: &[FunctionInfo], used_b: &mut HashSet<usize>) -> Result<Vec<FunctionMatch>> {
        let candidates: Vec<_> = functions_a.par_iter()
            .filter_map(|func_a| {
                let mut best_match: Option<(usize, f64, f64, MatchDetails)> = None;
                
                for (i, func_b) in functions_b.iter().enumerate() {
                    if used_b.contains(&i) {
                        continue;
                    }
                    
                    let (similarity, details) = self.calculate_detailed_similarity(func_a, func_b);
                    let confidence = self.calculate_confidence(func_a, func_b, similarity);
                    
                    if confidence >= self.confidence_threshold && similarity >= self.similarity_threshold {
                        if let Some((_, _, best_confidence, _)) = best_match {
                            if confidence > best_confidence {
                                best_match = Some((i, similarity, confidence, details));
                            }
                        } else {
                            best_match = Some((i, similarity, confidence, details));
                        }
                    }
                }
                
                best_match.map(|(idx, similarity, confidence, details)| {
                    (func_a.clone(), idx, similarity, confidence, details)
                })
            })
            .collect();
        
        // Add the best matches while avoiding conflicts
        let mut matches = Vec::new();
        for (func_a, idx, similarity, confidence, details) in candidates {
            if !used_b.contains(&idx) {
                let func_b = &functions_b[idx];
                matches.push(FunctionMatch {
                    function_a: func_a,
                    function_b: func_b.clone(),
                    similarity,
                    confidence,
                    match_type: MatchType::Heuristic,
                    details,
                });
                
                used_b.insert(idx);
            }
        }
        
        Ok(matches)
    }

    fn is_structurally_similar(&self, func_a: &FunctionInfo, func_b: &FunctionInfo) -> bool {
        // Check if functions have similar structure
        let bb_diff = (func_a.basic_blocks.len() as i32 - func_b.basic_blocks.len() as i32).abs();
        let complexity_diff = (func_a.cyclomatic_complexity as i32 - func_b.cyclomatic_complexity as i32).abs();
        let size_diff = (func_a.size as i64 - func_b.size as i64).abs() as f64 / func_a.size.max(func_b.size) as f64;
        
        bb_diff <= 2 && complexity_diff <= 2 && size_diff < 0.3
    }

    fn calculate_detailed_similarity(&self, func_a: &FunctionInfo, func_b: &FunctionInfo) -> (f64, MatchDetails) {
        // CFG similarity
        let cfg_similarity = if func_a.cfg_hash == func_b.cfg_hash { 1.0 } else { 0.0 };
        
        // Basic block similarity
        let bb_similarity = self.calculate_bb_similarity(func_a, func_b);
        
        // Instruction similarity
        let instruction_similarity = self.calculate_instruction_similarity(func_a, func_b);
        
        // Edge similarity
        let edge_similarity = self.calculate_edge_similarity(func_a, func_b);
        
        // Name similarity
        let name_similarity = self.calculate_name_similarity(&func_a.name, &func_b.name);
        
        // Call similarity
        let call_similarity = self.calculate_call_similarity(func_a, func_b);
        
        // Weighted similarity calculation (similar to BinDiff)
        let weighted_similarity = cfg_similarity * 0.5 + 
                                bb_similarity * 0.15 + 
                                instruction_similarity * 0.10 + 
                                edge_similarity * 0.25;
        
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

    fn calculate_bb_similarity(&self, func_a: &FunctionInfo, func_b: &FunctionInfo) -> f64 {
        let bb_count_a = func_a.basic_blocks.len();
        let bb_count_b = func_b.basic_blocks.len();
        
        if bb_count_a == 0 && bb_count_b == 0 {
            return 1.0;
        }
        
        if bb_count_a == 0 || bb_count_b == 0 {
            return 0.0;
        }
        
        let max_count = bb_count_a.max(bb_count_b);
        let min_count = bb_count_a.min(bb_count_b);
        
        min_count as f64 / max_count as f64
    }

    fn calculate_instruction_similarity(&self, func_a: &FunctionInfo, func_b: &FunctionInfo) -> f64 {
        let instr_count_a = func_a.instruction_count;
        let instr_count_b = func_b.instruction_count;
        
        if instr_count_a == 0 && instr_count_b == 0 {
            return 1.0;
        }
        
        if instr_count_a == 0 || instr_count_b == 0 {
            return 0.0;
        }
        
        let max_count = instr_count_a.max(instr_count_b);
        let min_count = instr_count_a.min(instr_count_b);
        
        min_count as f64 / max_count as f64
    }

    fn calculate_edge_similarity(&self, func_a: &FunctionInfo, func_b: &FunctionInfo) -> f64 {
        let complexity_a = func_a.cyclomatic_complexity;
        let complexity_b = func_b.cyclomatic_complexity;
        
        if complexity_a == 0 && complexity_b == 0 {
            return 1.0;
        }
        
        let max_complexity = complexity_a.max(complexity_b);
        let min_complexity = complexity_a.min(complexity_b);
        
        min_complexity as f64 / max_complexity as f64
    }

    fn calculate_name_similarity(&self, name_a: &str, name_b: &str) -> f64 {
        if name_a == name_b {
            1.0
        } else if name_a.contains(name_b) || name_b.contains(name_a) {
            0.7
        } else {
            // Simple character-based similarity
            let common_chars = name_a.chars().filter(|c| name_b.contains(*c)).count();
            let max_len = name_a.len().max(name_b.len());
            
            if max_len == 0 {
                0.0
            } else {
                common_chars as f64 / max_len as f64
            }
        }
    }

    fn calculate_call_similarity(&self, func_a: &FunctionInfo, func_b: &FunctionInfo) -> f64 {
        let call_count_a = func_a.call_count;
        let call_count_b = func_b.call_count;
        
        if call_count_a == 0 && call_count_b == 0 {
            return 1.0;
        }
        
        if call_count_a == 0 || call_count_b == 0 {
            return 0.0;
        }
        
        let max_count = call_count_a.max(call_count_b);
        let min_count = call_count_a.min(call_count_b);
        
        min_count as f64 / max_count as f64
    }

    fn calculate_confidence(&self, func_a: &FunctionInfo, func_b: &FunctionInfo, similarity: f64) -> f64 {
        let mut confidence = similarity;
        
        // Boost confidence for similar sizes
        let size_diff = (func_a.size as f64 - func_b.size as f64).abs() / func_a.size.max(func_b.size) as f64;
        if size_diff < 0.1 {
            confidence += 0.1;
        }
        
        // Boost confidence for similar complexity
        let complexity_diff = (func_a.cyclomatic_complexity as i32 - func_b.cyclomatic_complexity as i32).abs();
        if complexity_diff < 2 {
            confidence += 0.1;
        }
        
        // Boost confidence for similar basic block count
        let bb_diff = (func_a.basic_blocks.len() as i32 - func_b.basic_blocks.len() as i32).abs();
        if bb_diff < 2 {
            confidence += 0.1;
        }
        
        // Boost confidence for same name
        if func_a.name == func_b.name {
            confidence += 0.2;
        }
        
        confidence.min(1.0)
    }

    pub fn perform_diff_mock(&self, binary_a_name: &str, binary_b_name: &str) -> Result<DiffResult> {
        let start_time = Instant::now();
        
        info!("Starting binary diff analysis");
        
        // Extract functions from both binaries (mock)
        let functions_a = self.extract_function_info_mock(binary_a_name)?;
        let functions_b = self.extract_function_info_mock(binary_b_name)?;
        
        info!("Extracted {} functions from binary A, {} from binary B", functions_a.len(), functions_b.len());
        
        // Perform matching
        let matches = self.compare_functions(&functions_a, &functions_b)?;
        
        // Find unmatched functions
        let matched_a: HashSet<u64> = matches.iter().map(|m| m.function_a.address).collect();
        let matched_b: HashSet<u64> = matches.iter().map(|m| m.function_b.address).collect();
        
        let unmatched_a: Vec<FunctionInfo> = functions_a.into_iter()
            .filter(|f| !matched_a.contains(&f.address))
            .collect();
        
        let unmatched_b: Vec<FunctionInfo> = functions_b.into_iter()
            .filter(|f| !matched_b.contains(&f.address))
            .collect();
        
        // Calculate overall similarity
        let similarity_score = if !matches.is_empty() {
            matches.iter().map(|m| m.similarity).sum::<f64>() / matches.len() as f64
        } else {
            0.0
        };
        
        let analysis_time = start_time.elapsed().as_secs_f64();
        
        info!("Diff analysis completed in {:.2}s: {} matches, similarity: {:.3}", 
              analysis_time, matches.len(), similarity_score);
        
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

// C FFI exports for Binary Ninja integration
#[no_mangle]
pub extern "C" fn rust_diff_init() -> *mut BinaryDiffEngine {
    let _ = env_logger::try_init();
    info!("Initializing Rust Diff engine");
    
    let engine = Box::new(BinaryDiffEngine::new());
    Box::into_raw(engine)
}

#[no_mangle]
pub extern "C" fn rust_diff_cleanup(engine: *mut BinaryDiffEngine) {
    if !engine.is_null() {
        unsafe {
            let _ = Box::from_raw(engine);
        }
    }
}

#[no_mangle]
pub extern "C" fn rust_diff_perform_diff_mock(
    engine: *mut BinaryDiffEngine,
    binary_a_name: *const c_char,
    binary_b_name: *const c_char,
) -> *mut DiffResult {
    if engine.is_null() || binary_a_name.is_null() || binary_b_name.is_null() {
        return std::ptr::null_mut();
    }
    
    let engine = unsafe { &mut *engine };
    let binary_a_name = unsafe { CStr::from_ptr(binary_a_name) };
    let binary_b_name = unsafe { CStr::from_ptr(binary_b_name) };
    
    let binary_a_name = match binary_a_name.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    
    let binary_b_name = match binary_b_name.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    
    match engine.perform_diff_mock(binary_a_name, binary_b_name) {
        Ok(result) => Box::into_raw(Box::new(result)),
        Err(e) => {
            error!("Diff failed: {}", e);
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn rust_diff_free_result(result: *mut DiffResult) {
    if !result.is_null() {
        unsafe {
            let _ = Box::from_raw(result);
        }
    }
}

#[no_mangle]
pub extern "C" fn rust_diff_get_match_count(result: *const DiffResult) -> usize {
    if result.is_null() {
        return 0;
    }
    
    let result = unsafe { &*result };
    result.matched_functions.len()
}

#[no_mangle]
pub extern "C" fn rust_diff_get_similarity_score(result: *const DiffResult) -> f64 {
    if result.is_null() {
        return 0.0;
    }
    
    let result = unsafe { &*result };
    result.similarity_score
}

#[no_mangle]
pub extern "C" fn rust_diff_save_results(
    result: *const DiffResult,
    output_path: *const c_char,
) -> i32 {
    if result.is_null() || output_path.is_null() {
        return -1;
    }
    
    let result = unsafe { &*result };
    let output_path = unsafe { CStr::from_ptr(output_path) };
    let output_path = match output_path.to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    
    let engine = BinaryDiffEngine::new();
    match engine.save_results(result, output_path) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}