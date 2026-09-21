use std::path::PathBuf;
use tiktoken_rs::CoreBPE;

use crate::graph::RankedFile;
use crate::{RepomapError, Result, SymbolKind};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SelectedTag {
    pub name: String,
    pub kind: SymbolKind,
    pub line: usize,
    pub signature: String,
    pub score: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SelectedFile {
    pub path: PathBuf,
    pub score: f64,
    pub definitions: Vec<SelectedTag>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct MapBudgetPlan {
    pub files: Vec<SelectedFile>,
    pub total_tokens: usize,
    pub max_tokens: usize,
}

fn get_bpe(encoding: &str) -> Result<CoreBPE> {
    if encoding == "o200k_base" {
        tiktoken_rs::o200k_base().map_err(|e| RepomapError::Tokenizer(e.to_string()))
    } else {
        tiktoken_rs::cl100k_base().map_err(|e| RepomapError::Tokenizer(e.to_string()))
    }
}

pub fn count_tokens(text: &str, encoding: &str) -> Result<usize> {
    let bpe = get_bpe(encoding)?;
    Ok(bpe.encode_ordinary(text).len())
}

fn render_plan_preview(files: &[SelectedFile]) -> String {
    let mut out = String::new();
    for (i, file) in files.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let clean_path = file.path.to_string_lossy();
        out.push_str(&clean_path);
        out.push_str(":\n");
        for def in &file.definitions {
            out.push_str("│⋮...\n│");
            out.push_str(&def.signature);
            out.push('\n');
        }
    }
    out
}

pub fn fit_to_budget(
    ranked_files: &[RankedFile],
    max_tokens: usize,
    encoding: &str,
) -> Result<MapBudgetPlan> {
    if max_tokens == 0 || ranked_files.is_empty() {
        return Ok(MapBudgetPlan {
            files: Vec::new(),
            total_tokens: 0,
            max_tokens,
        });
    }

    let bpe = get_bpe(encoding)?;
    let mut plan_files: Vec<SelectedFile> = Vec::new();

    for rf in ranked_files {
        // Prepare a candidate entry for this file
        let mut candidate_file = SelectedFile {
            path: rf.path.clone(),
            score: rf.score,
            definitions: Vec::new(),
        };

        // First test if just adding the file (with or without its first definition) fits.
        // Even an empty file entry takes tokens: "path:\n"
        // Let's test if candidate_file with 0 definitions can be added.
        let mut test_files = plan_files.clone();
        test_files.push(candidate_file.clone());
        let candidate_preview = render_plan_preview(&test_files);
        let tokens = bpe.encode_ordinary(&candidate_preview).len();
        if tokens > max_tokens {
            // Cannot even add this file's header, skip to next file (or stop)
            continue;
        }

        // Try adding definitions greedily
        for ranked_tag in &rf.ranked_definitions {
            let selected_tag = SelectedTag {
                name: ranked_tag.tag.name.clone(),
                kind: ranked_tag.tag.kind.clone(),
                line: ranked_tag.tag.line,
                signature: ranked_tag.tag.signature.clone(),
                score: ranked_tag.score,
            };

            let mut def_test_files = plan_files.clone();
            let mut file_with_def = candidate_file.clone();
            file_with_def.definitions.push(selected_tag.clone());
            def_test_files.push(file_with_def.clone());

            let preview = render_plan_preview(&def_test_files);
            let def_tokens = bpe.encode_ordinary(&preview).len();
            if def_tokens <= max_tokens {
                candidate_file = file_with_def;
            }
        }

        // If candidate_file has definitions, or if ranked_definitions was empty but header fits,
        // add candidate_file to plan_files.
        // Wait, if a file had definitions but none of them fit, do we keep the file header?
        // Let's check: if candidate_file.definitions is not empty, or if rf.ranked_definitions was empty,
        // we keep it. If rf had definitions but NONE fit, candidate_file without definitions takes budget.
        // Usually, keeping empty file header is acceptable only if it fits. But candidate_preview already <= max_tokens!
        // To be safe and clean: only add candidate_file if it contains definitions OR if rf had no definitions at all.
        if !candidate_file.definitions.is_empty() || rf.ranked_definitions.is_empty() {
            plan_files.push(candidate_file);
        }
    }

    let final_preview = render_plan_preview(&plan_files);
    let final_tokens = if plan_files.is_empty() {
        0
    } else {
        bpe.encode_ordinary(&final_preview).len()
    };

    Ok(MapBudgetPlan {
        files: plan_files,
        total_tokens: final_tokens,
        max_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tag;
    use crate::graph::RankedTag;

    fn make_ranked_file(path: &str, score: f64, def_names: &[(&str, usize, &str)]) -> RankedFile {
        let ranked_definitions = def_names
            .iter()
            .enumerate()
            .map(|(i, (name, line, sig))| RankedTag {
                tag: Tag {
                    name: name.to_string(),
                    kind: SymbolKind::Function,
                    line: *line,
                    signature: sig.to_string(),
                },
                score: score * (1.0 + (def_names.len() - i) as f64),
            })
            .collect();

        RankedFile {
            path: PathBuf::from(path),
            score,
            ranked_definitions,
        }
    }

    #[test]
    fn test_token_counting() {
        let text = "fn calculate_hash(data: &[u8]) -> u64";
        let tokens_cl100k = count_tokens(text, "cl100k_base").expect("cl100k counting");
        let tokens_o200k = count_tokens(text, "o200k_base").expect("o200k counting");

        assert!(tokens_cl100k > 0);
        assert!(tokens_o200k > 0);
        assert_eq!(
            count_tokens("", "cl100k_base").unwrap(),
            0,
            "Empty string should have 0 tokens"
        );
    }

    #[test]
    fn test_fit_to_budget_zero_tokens() {
        let files = vec![make_ranked_file(
            "src/main.rs",
            1.0,
            &[("main", 1, "fn main()")],
        )];
        let plan = fit_to_budget(&files, 0, "cl100k_base").unwrap();
        assert_eq!(plan.total_tokens, 0);
        assert_eq!(plan.max_tokens, 0);
        assert!(plan.files.is_empty());
    }

    #[test]
    fn test_fit_to_budget_strict_bound() {
        let files = vec![
            make_ranked_file(
                "src/a.rs",
                1.0,
                &[
                    (
                        "func_a1",
                        10,
                        "pub fn func_a1(param: &str) -> Result<String, Error>",
                    ),
                    ("func_a2", 20, "pub fn func_a2(x: usize, y: usize) -> usize"),
                    ("func_a3", 30, "pub fn func_a3() -> ()"),
                ],
            ),
            make_ranked_file(
                "src/b.rs",
                0.8,
                &[
                    ("func_b1", 5, "pub fn func_b1<T: Clone>(item: T) -> Vec<T>"),
                    ("func_b2", 15, "pub fn func_b2()"),
                ],
            ),
            make_ranked_file("src/c.rs", 0.5, &[("func_c1", 1, "pub fn func_c1()")]),
        ];

        for budget in [5, 10, 20, 50, 100, 200, 1000] {
            let plan = fit_to_budget(&files, budget, "cl100k_base").unwrap();
            assert!(
                plan.total_tokens <= budget,
                "Plan tokens ({}) exceeded max_tokens ({})",
                plan.total_tokens,
                budget
            );
        }
    }

    #[test]
    fn test_fit_to_budget_prioritizes_high_scores() {
        let files = vec![
            make_ranked_file("src/high.rs", 0.9, &[("high_fn", 1, "pub fn high_fn()")]),
            make_ranked_file("src/low.rs", 0.1, &[("low_fn", 1, "pub fn low_fn()")]),
        ];

        // Give a budget that fits only the first file + definition
        let plan_small = fit_to_budget(&files, 25, "cl100k_base").unwrap();
        assert_eq!(plan_small.files.len(), 1);
        assert_eq!(plan_small.files[0].path, PathBuf::from("src/high.rs"));
        assert_eq!(plan_small.files[0].definitions.len(), 1);
        assert_eq!(plan_small.files[0].definitions[0].name, "high_fn");

        // Give a generous budget that fits both
        let plan_large = fit_to_budget(&files, 100, "cl100k_base").unwrap();
        assert_eq!(plan_large.files.len(), 2);
        assert_eq!(plan_large.files[0].path, PathBuf::from("src/high.rs"));
        assert_eq!(plan_large.files[1].path, PathBuf::from("src/low.rs"));
    }
}
