use crate::Result;
use crate::budget::MapBudgetPlan;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Aider,
    Json,
}

pub mod aider;
pub mod json;

pub fn format_repomap(plan: &MapBudgetPlan, format: OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Aider => Ok(aider::format_aider(plan)),
        OutputFormat::Json => json::format_json(plan),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SymbolKind;
    use crate::budget::{SelectedFile, SelectedTag};
    use std::path::PathBuf;

    #[test]
    fn test_format_aider_empty_plan() {
        let plan = MapBudgetPlan {
            files: Vec::new(),
            total_tokens: 0,
            max_tokens: 1024,
        };

        let output = format_repomap(&plan, OutputFormat::Aider).expect("formatting empty plan");
        assert_eq!(output, "");
    }

    #[test]
    fn test_format_aider_single_and_multiple_files() {
        let plan = MapBudgetPlan {
            files: vec![
                SelectedFile {
                    path: PathBuf::from("src/main.rs"),
                    score: 1.0,
                    definitions: vec![
                        SelectedTag {
                            name: "helper".to_string(),
                            kind: SymbolKind::Function,
                            line: 42,
                            signature: "fn helper() -> bool".to_string(),
                            score: 0.5,
                        },
                        SelectedTag {
                            name: "main".to_string(),
                            kind: SymbolKind::Function,
                            line: 10,
                            signature: "fn main()".to_string(),
                            score: 1.0,
                        },
                    ],
                },
                SelectedFile {
                    path: PathBuf::from("src/lib.rs"),
                    score: 0.8,
                    definitions: vec![SelectedTag {
                        name: "Config".to_string(),
                        kind: SymbolKind::Struct,
                        line: 5,
                        signature: "pub struct Config".to_string(),
                        score: 0.8,
                    }],
                },
            ],
            total_tokens: 50,
            max_tokens: 100,
        };

        let output = format_repomap(&plan, OutputFormat::Aider).expect("formatting aider");

        let expected = "src/main.rs:\n\
│⋮...\n\
│fn main()\n\
│⋮...\n\
│fn helper() -> bool\n\
\n\
src/lib.rs:\n\
│⋮...\n\
│pub struct Config\n";

        assert_eq!(output, expected);
    }

    #[test]
    fn test_format_json_valid_syntax() {
        let plan = MapBudgetPlan {
            files: vec![SelectedFile {
                path: PathBuf::from("src/test.rs"),
                score: 0.9,
                definitions: vec![SelectedTag {
                    name: "test_fn".to_string(),
                    kind: SymbolKind::Function,
                    line: 15,
                    signature: "pub fn test_fn()".to_string(),
                    score: 0.9,
                }],
            }],
            total_tokens: 25,
            max_tokens: 1024,
        };

        let json_str = format_repomap(&plan, OutputFormat::Json).expect("formatting json");

        let val: serde_json::Value = serde_json::from_str(&json_str).expect("parse JSON");
        assert_eq!(val["total_tokens"], 25);
        assert_eq!(val["max_tokens"], 1024);
        assert!(val["files"].is_array());
        assert_eq!(val["files"].as_array().unwrap().len(), 1);
        assert_eq!(val["files"][0]["path"], "src/test.rs");
        assert_eq!(val["files"][0]["definitions"][0]["name"], "test_fn");
        assert_eq!(val["files"][0]["definitions"][0]["line"], 15);
        assert_eq!(
            val["files"][0]["definitions"][0]["signature"],
            "pub fn test_fn()"
        );
    }
}
