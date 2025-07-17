use crate::{FunctionInfo, BasicBlockInfo, InstructionInfo, FunctionMatch, DiffResult};
use anyhow::{Result, Context};
use serde::{Serialize, Deserialize};
use std::path::Path;
use std::fs;
use std::ffi::CString;
use std::os::raw::c_char;

#[derive(Debug, Serialize, Deserialize)]
pub struct DiffDatabase {
    pub binary_a_path: String,
    pub binary_b_path: String,
    pub functions_a: Vec<FunctionInfo>,
    pub functions_b: Vec<FunctionInfo>,
    pub matches: Vec<FunctionMatch>,
    pub metadata: DatabaseMetadata,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DatabaseMetadata {
    pub created_at: String,
    pub plugin_version: String,
    pub binary_a_hash: String,
    pub binary_b_hash: String,
    pub total_functions_a: usize,
    pub total_functions_b: usize,
    pub total_matches: usize,
    pub analysis_time_seconds: f64,
}

pub struct DatabaseManager;

impl DatabaseManager {
    /// Save diff results to a JSON file
    pub fn save_diff_results(
        diff_result: &DiffResult,
        binary_a_path: &str,
        binary_b_path: &str,
        output_path: &Path,
    ) -> Result<()> {
        let metadata = DatabaseMetadata {
            created_at: chrono::Utc::now().to_rfc3339(),
            plugin_version: env!("CARGO_PKG_VERSION").to_string(),
            binary_a_hash: "".to_string(), // TODO: Calculate actual hash
            binary_b_hash: "".to_string(), // TODO: Calculate actual hash
            total_functions_a: diff_result.matched_functions.len() + diff_result.unmatched_functions_a.len(),
            total_functions_b: diff_result.matched_functions.len() + diff_result.unmatched_functions_b.len(),
            total_matches: diff_result.matched_functions.len(),
            analysis_time_seconds: 0.0, // TODO: Track actual time
        };

        let database = DiffDatabase {
            binary_a_path: binary_a_path.to_string(),
            binary_b_path: binary_b_path.to_string(),
            functions_a: diff_result.matched_functions.iter()
                .map(|m| m.function_a.clone())
                .chain(diff_result.unmatched_functions_a.iter().cloned())
                .collect(),
            functions_b: diff_result.matched_functions.iter()
                .map(|m| m.function_b.clone())
                .chain(diff_result.unmatched_functions_b.iter().cloned())
                .collect(),
            matches: diff_result.matched_functions.clone(),
            metadata,
        };

        let json_data = serde_json::to_string_pretty(&database)
            .context("Failed to serialize diff results")?;

        fs::write(output_path, json_data)
            .context("Failed to write database file")?;

        Ok(())
    }

    /// Load diff results from a JSON file
    pub fn load_diff_results(input_path: &Path) -> Result<DiffDatabase> {
        let json_data = fs::read_to_string(input_path)
            .context("Failed to read database file")?;

        let database: DiffDatabase = serde_json::from_str(&json_data)
            .context("Failed to deserialize diff results")?;

        Ok(database)
    }

    /// Export results to CSV format
    pub fn export_to_csv(database: &DiffDatabase, output_path: &Path) -> Result<()> {
        let mut csv_content = String::new();
        
        // CSV header
        csv_content.push_str("Function A,Address A,Function B,Address B,Similarity,Confidence,Match Type,Size A,Size B,BB Count A,BB Count B,Instr Count A,Instr Count B\n");
        
        // Add matched functions
        for match_result in &database.matches {
            csv_content.push_str(&format!(
                "{},{:x},{},{:x},{:.4},{:.4},{:?},{},{},{},{},{},{}\n",
                match_result.function_a.name,
                match_result.function_a.address,
                match_result.function_b.name,
                match_result.function_b.address,
                match_result.similarity,
                match_result.confidence,
                match_result.match_type,
                match_result.function_a.size,
                match_result.function_b.size,
                match_result.function_a.basic_blocks.len(),
                match_result.function_b.basic_blocks.len(),
                match_result.function_a.instructions.len(),
                match_result.function_b.instructions.len()
            ));
        }
        
        fs::write(output_path, csv_content)
            .context("Failed to write CSV file")?;
        
        Ok(())
    }

    /// Export results to SQLite database
    pub fn export_to_sqlite(database: &DiffDatabase, output_path: &Path) -> Result<()> {
        // For now, create a simple SQL script that can be imported
        let mut sql_content = String::new();
        
        // Create table
        sql_content.push_str("CREATE TABLE IF NOT EXISTS function_matches (\n");
        sql_content.push_str("    id INTEGER PRIMARY KEY AUTOINCREMENT,\n");
        sql_content.push_str("    function_a_name TEXT,\n");
        sql_content.push_str("    function_a_address INTEGER,\n");
        sql_content.push_str("    function_b_name TEXT,\n");
        sql_content.push_str("    function_b_address INTEGER,\n");
        sql_content.push_str("    similarity REAL,\n");
        sql_content.push_str("    confidence REAL,\n");
        sql_content.push_str("    match_type TEXT,\n");
        sql_content.push_str("    size_a INTEGER,\n");
        sql_content.push_str("    size_b INTEGER,\n");
        sql_content.push_str("    bb_count_a INTEGER,\n");
        sql_content.push_str("    bb_count_b INTEGER,\n");
        sql_content.push_str("    instr_count_a INTEGER,\n");
        sql_content.push_str("    instr_count_b INTEGER,\n");
        sql_content.push_str("    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP\n");
        sql_content.push_str(");\n\n");
        
        // Insert data
        for match_result in &database.matches {
            sql_content.push_str(&format!(
                "INSERT INTO function_matches (function_a_name, function_a_address, function_b_name, function_b_address, similarity, confidence, match_type, size_a, size_b, bb_count_a, bb_count_b, instr_count_a, instr_count_b) VALUES ('{}', {}, '{}', {}, {:.4}, {:.4}, '{:?}', {}, {}, {}, {}, {}, {});\n",
                match_result.function_a.name.replace("'", "''"),
                match_result.function_a.address,
                match_result.function_b.name.replace("'", "''"),
                match_result.function_b.address,
                match_result.similarity,
                match_result.confidence,
                match_result.match_type,
                match_result.function_a.size,
                match_result.function_b.size,
                match_result.function_a.basic_blocks.len(),
                match_result.function_b.basic_blocks.len(),
                match_result.function_a.instructions.len(),
                match_result.function_b.instructions.len()
            ));
        }
        
        fs::write(output_path, sql_content)
            .context("Failed to write SQL file")?;
        
        Ok(())
    }

    /// Export results to HTML report
    pub fn export_to_html(database: &DiffDatabase, output_path: &Path) -> Result<()> {
        let html_content = format!(
            r#"
<!DOCTYPE html>
<html>
<head>
    <title>Binary Diff Report</title>
    <style>
        body {{ font-family: Arial, sans-serif; margin: 20px; }}
        .header {{ background-color: #f0f0f0; padding: 20px; margin-bottom: 20px; }}
        .summary {{ background-color: #e8f4f8; padding: 15px; margin-bottom: 20px; }}
        .matches {{ margin-bottom: 20px; }}
        table {{ border-collapse: collapse; width: 100%; }}
        th, td {{ border: 1px solid #ddd; padding: 8px; text-align: left; }}
        th {{ background-color: #4CAF50; color: white; }}
        tr:nth-child(even) {{ background-color: #f2f2f2; }}
        .exact {{ background-color: #90EE90; color: #006400; }}
        .structural {{ background-color: #FFD700; color: #8B4513; }}
        .heuristic {{ background-color: #FFB6C1; color: #8B0000; }}
    </style>
</head>
<body>
    <div class="header">
        <h1>Binary Diff Report</h1>
        <p>Generated by Rust Diff Plugin v{}</p>
        <p>Created: {}</p>
    </div>
    
    <div class="summary">
        <h2>Summary</h2>
        <p><strong>Binary A:</strong> {}</p>
        <p><strong>Binary B:</strong> {}</p>
        <p><strong>Total Functions A:</strong> {}</p>
        <p><strong>Total Functions B:</strong> {}</p>
        <p><strong>Total Matches:</strong> {}</p>
        <p><strong>Analysis Time:</strong> {:.2} seconds</p>
    </div>
    
    <div class="matches">
        <h2>Function Matches</h2>
        <table>
            <tr>
                <th>Function A</th>
                <th>Address A</th>
                <th>Function B</th>
                <th>Address B</th>
                <th>Similarity</th>
                <th>Confidence</th>
                <th>Match Type</th>
            </tr>
            {}
        </table>
    </div>
</body>
</html>
"#,
            database.metadata.plugin_version,
            database.metadata.created_at,
            database.binary_a_path,
            database.binary_b_path,
            database.metadata.total_functions_a,
            database.metadata.total_functions_b,
            database.metadata.total_matches,
            database.metadata.analysis_time_seconds,
            Self::generate_html_table_rows(&database.matches)
        );

        fs::write(output_path, html_content)
            .context("Failed to write HTML file")?;

        Ok(())
    }

    /// Generate HTML table rows for matches
    fn generate_html_table_rows(matches: &[FunctionMatch]) -> String {
        let mut rows = String::new();
        
        for match_result in matches {
            let class = match match_result.match_type {
                crate::MatchType::Exact => "exact",
                crate::MatchType::Structural => "structural",
                crate::MatchType::Heuristic => "heuristic",
                crate::MatchType::Manual => "manual",
            };
            
            rows.push_str(&format!(
                r#"<tr class="{}">
                    <td>{}</td>
                    <td>0x{:x}</td>
                    <td>{}</td>
                    <td>0x{:x}</td>
                    <td>{:.4}</td>
                    <td>{:.4}</td>
                    <td>{:?}</td>
                </tr>"#,
                class,
                match_result.function_a.name,
                match_result.function_a.address,
                match_result.function_b.name,
                match_result.function_b.address,
                match_result.similarity,
                match_result.confidence,
                match_result.match_type
            ));
        }
        
        rows
    }

    /// Generate statistics from diff results
    pub fn generate_statistics(database: &DiffDatabase) -> DiffStatistics {
        let mut exact_matches = 0;
        let mut structural_matches = 0;
        let mut heuristic_matches = 0;
        let mut manual_matches = 0;
        
        let mut similarity_sum = 0.0;
        let mut confidence_sum = 0.0;
        
        for match_result in &database.matches {
            similarity_sum += match_result.similarity;
            confidence_sum += match_result.confidence;
            
            match match_result.match_type {
                crate::MatchType::Exact => exact_matches += 1,
                crate::MatchType::Structural => structural_matches += 1,
                crate::MatchType::Heuristic => heuristic_matches += 1,
                crate::MatchType::Manual => manual_matches += 1,
            }
        }
        
        let total_matches = database.matches.len();
        let average_similarity = if total_matches > 0 {
            similarity_sum / total_matches as f64
        } else {
            0.0
        };
        
        let average_confidence = if total_matches > 0 {
            confidence_sum / total_matches as f64
        } else {
            0.0
        };
        
        DiffStatistics {
            total_matches,
            exact_matches,
            structural_matches,
            heuristic_matches,
            manual_matches,
            average_similarity,
            average_confidence,
            unmatched_functions_a: database.metadata.total_functions_a - total_matches,
            unmatched_functions_b: database.metadata.total_functions_b - total_matches,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiffStatistics {
    pub total_matches: usize,
    pub exact_matches: usize,
    pub structural_matches: usize,
    pub heuristic_matches: usize,
    pub manual_matches: usize,
    pub average_similarity: f64,
    pub average_confidence: f64,
    pub unmatched_functions_a: usize,
    pub unmatched_functions_b: usize,
}

impl DiffStatistics {
    /// Print statistics to console
    pub fn print_summary(&self) {
        println!("=== Diff Statistics ===");
        println!("Total Matches: {}", self.total_matches);
        println!("  - Exact: {}", self.exact_matches);
        println!("  - Structural: {}", self.structural_matches);
        println!("  - Heuristic: {}", self.heuristic_matches);
        println!("  - Manual: {}", self.manual_matches);
        println!("Average Similarity: {:.4}", self.average_similarity);
        println!("Average Confidence: {:.4}", self.average_confidence);
        println!("Unmatched Functions A: {}", self.unmatched_functions_a);
        println!("Unmatched Functions B: {}", self.unmatched_functions_b);
    }
}