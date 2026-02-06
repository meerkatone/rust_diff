use crate::types::*;
use anyhow::Result;
use log::info;
use std::hash::Hash;

/// Generate mock functions for testing/demo purposes
pub fn generate_mock_functions(binary_name: &str) -> Result<Vec<FunctionInfo>> {
    info!("Extracting function information from binary: {}", binary_name);

    let mut functions = Vec::new();

    let function_patterns = vec![
        ("main", 0x1000u64, 200u64, 3usize, 5u32),
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

        for bb_idx in 0..*bb_count {
            let bb_addr = base_addr + (bb_idx * 40) as u64;
            let mut instructions = Vec::new();

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

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        Hash::hash(&format!("{}_{}", name, bb_count), &mut hasher);
        let cfg_hash = format!("cfg_{:x}", std::hash::Hasher::finish(&hasher));

        let mut call_hasher = std::collections::hash_map::DefaultHasher::new();
        Hash::hash(&format!("{}_{}", name, complexity), &mut call_hasher);
        let call_graph_hash = format!("call_{:x}", std::hash::Hasher::finish(&call_hasher));

        let function = FunctionInfo {
            name: name.to_string(),
            address: *base_addr,
            size: *size,
            basic_blocks,
            instructions: all_instructions,
            cyclomatic_complexity: *complexity,
            call_graph_hash,
            cfg_hash,
            instruction_count: (bb_count * 3),
            call_count: if *complexity > 2 { 2 } else { 1 },
        };
        functions.push(function);
    }

    info!("Extracted {} functions", functions.len());
    Ok(functions)
}
