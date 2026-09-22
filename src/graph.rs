use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::{FileTags, Tag};

#[derive(Debug, Clone, PartialEq)]
pub struct RankedTag {
    pub tag: Tag,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RankedFile {
    pub path: PathBuf,
    pub score: f64,
    pub ranked_definitions: Vec<RankedTag>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RankingStrategy {
    Legacy,
    Balanced,
}

#[derive(Debug, Clone)]
pub struct RepoGraph {
    file_tags: Vec<FileTags>,
    adj_legacy: Vec<Vec<usize>>,
    adj_balanced: Vec<Vec<(usize, f64)>>,
    symbol_ref_counts: HashMap<String, usize>,
    symbol_def_file_counts: HashMap<String, usize>,
}

impl RepoGraph {
    /// Construct a reference dependency graph from extracted file tags.
    pub fn from_file_tags(file_tags: &[FileTags]) -> Self {
        let n = file_tags.len();

        // 1. Identify which files define each symbol name (file index deduplicated)
        let mut symbol_def_files: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, ft) in file_tags.iter().enumerate() {
            let mut seen_in_file = HashSet::new();
            for def in &ft.definitions {
                if seen_in_file.insert(&def.name) {
                    symbol_def_files
                        .entry(def.name.clone())
                        .or_default()
                        .push(idx);
                }
            }
        }

        // 2. Track how many times each symbol S is referenced across all files
        let mut symbol_ref_counts: HashMap<String, usize> = HashMap::new();
        for ft in file_tags {
            let mut sorted_refs: Vec<&String> = ft.references.iter().collect();
            sorted_refs.sort();
            for r in sorted_refs {
                *symbol_ref_counts.entry(r.clone()).or_default() += 1;
            }
        }

        let symbol_def_file_counts: HashMap<String, usize> = symbol_def_files
            .iter()
            .map(|(k, v)| (k.clone(), v.len()))
            .collect();

        // 3. Build both adjacency variants:
        // - legacy: unweighted multigraph (previous behavior)
        // - balanced: weighted graph reducing ambiguous/common symbol dominance
        let mut adj_legacy: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut adj_balanced: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
        for (i, ft) in file_tags.iter().enumerate() {
            let mut sorted_refs: Vec<&String> = ft.references.iter().collect();
            sorted_refs.sort();
            let mut weighted_targets: HashMap<usize, f64> = HashMap::new();
            for r in sorted_refs {
                if let Some(def_files) = symbol_def_files.get(r) {
                    let ambiguity_penalty = 1.0 / (def_files.len() as f64);
                    let ref_count = symbol_ref_counts.get(r).copied().unwrap_or(0) as f64;
                    let popularity_penalty = 1.0 / (1.0 + ref_count.ln_1p());
                    let contribution = ambiguity_penalty * popularity_penalty;
                    for &j in def_files {
                        if j != i {
                            adj_legacy[i].push(j);
                            *weighted_targets.entry(j).or_default() += contribution;
                        }
                    }
                }
            }
            adj_legacy[i].sort();

            let mut weighted_vec: Vec<(usize, f64)> = weighted_targets.into_iter().collect();
            weighted_vec.sort_by_key(|(target, _)| *target);
            adj_balanced[i] = weighted_vec;
        }

        Self {
            file_tags: file_tags.to_vec(),
            adj_legacy,
            adj_balanced,
            symbol_ref_counts,
            symbol_def_file_counts,
        }
    }

    /// Compute legacy PageRank or Personalized PageRank (if focus files are specified).
    pub fn compute_pagerank(&self, focus: &[PathBuf]) -> Vec<RankedFile> {
        self.compute_ranked_files(focus, RankingStrategy::Legacy)
    }

    /// Compute ranking with a strategy selector.
    pub fn compute_ranked_files(
        &self,
        focus: &[PathBuf],
        ranking_strategy: RankingStrategy,
    ) -> Vec<RankedFile> {
        let file_scores = self.compute_file_scores(focus, ranking_strategy);
        if file_scores.is_empty() {
            return Vec::new();
        }

        let n = self.file_tags.len();
        let mut ranked_files = Vec::with_capacity(n);
        for (i, ft) in self.file_tags.iter().enumerate() {
            let file_score = file_scores[i];
            let mut ranked_definitions = Vec::with_capacity(ft.definitions.len());

            for def in &ft.definitions {
                let ref_count = self.symbol_ref_counts.get(&def.name).copied().unwrap_or(0) as f64;
                let tag_score = match ranking_strategy {
                    RankingStrategy::Legacy => file_score * (1.0 + ref_count),
                    RankingStrategy::Balanced => {
                        let def_file_count = self
                            .symbol_def_file_counts
                            .get(&def.name)
                            .copied()
                            .unwrap_or(1) as f64;
                        let rarity_signal = ref_count.ln_1p();
                        file_score * (1.0 + (rarity_signal / def_file_count))
                    }
                };
                ranked_definitions.push(RankedTag {
                    tag: def.clone(),
                    score: tag_score,
                });
            }

            // Sort definitions descending by score, ties broken by line number ascending
            ranked_definitions.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.tag.line.cmp(&b.tag.line))
                    .then_with(|| a.tag.name.cmp(&b.tag.name))
            });

            ranked_files.push(RankedFile {
                path: ft.path.clone(),
                score: file_score,
                ranked_definitions,
            });
        }

        ranked_files.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.path.cmp(&b.path))
        });

        ranked_files
    }

    fn compute_file_scores(
        &self,
        focus: &[PathBuf],
        ranking_strategy: RankingStrategy,
    ) -> Vec<f64> {
        let n = self.file_tags.len();
        if n == 0 {
            return Vec::new();
        }

        // 1. Determine teleport vector v
        let focus_indices: Vec<usize> = (0..n)
            .filter(|&i| is_in_focus(&self.file_tags[i].path, focus))
            .collect();

        let v = if !focus_indices.is_empty() {
            let mut vec = vec![0.0; n];
            let mass = 1.0 / (focus_indices.len() as f64);
            for &idx in &focus_indices {
                vec[idx] = mass;
            }
            vec
        } else {
            vec![1.0 / (n as f64); n]
        };

        let damping = 0.85;
        let max_iterations = 100;
        let tolerance = 1e-6;

        let mut p = v.clone();

        for _ in 0..max_iterations {
            let mut dangling_sum = 0.0;
            let mut m_p = vec![0.0; n];

            match ranking_strategy {
                RankingStrategy::Legacy => {
                    for (edges, &p_i) in self.adj_legacy.iter().zip(p.iter()) {
                        let out_degree = edges.len();
                        if out_degree == 0 {
                            dangling_sum += p_i;
                        } else {
                            let share = p_i / (out_degree as f64);
                            for &target in edges {
                                m_p[target] += share;
                            }
                        }
                    }
                }
                RankingStrategy::Balanced => {
                    for (edges, &p_i) in self.adj_balanced.iter().zip(p.iter()) {
                        if edges.is_empty() {
                            dangling_sum += p_i;
                            continue;
                        }

                        let total_weight: f64 = edges.iter().map(|(_, weight)| *weight).sum();
                        if total_weight <= f64::EPSILON {
                            dangling_sum += p_i;
                            continue;
                        }

                        for &(target, weight) in edges {
                            m_p[target] += p_i * (weight / total_weight);
                        }
                    }
                }
            }

            let p_next: Vec<f64> = v
                .iter()
                .zip(m_p.iter())
                .map(|(&v_i, &m_p_i)| {
                    (1.0 - damping) * v_i + damping * (m_p_i + dangling_sum * v_i)
                })
                .collect();

            let diff: f64 = p
                .iter()
                .zip(p_next.iter())
                .map(|(a, b)| (a - b).abs())
                .sum();

            p = p_next;

            if diff < tolerance {
                break;
            }
        }

        p
    }
}

fn is_in_focus(file_path: &Path, focus: &[PathBuf]) -> bool {
    let clean_file = file_path.strip_prefix("./").unwrap_or(file_path);
    focus.iter().any(|f| {
        if f == file_path {
            return true;
        }
        let clean_f = f.strip_prefix("./").unwrap_or(f);
        clean_file == clean_f
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SymbolKind;

    fn make_tag(name: &str, line: usize) -> Tag {
        Tag {
            name: name.to_string(),
            kind: SymbolKind::Function,
            line,
            signature: format!("fn {}()", name),
        }
    }

    #[test]
    fn test_graph_empty_and_single_file() {
        // Empty graph
        let empty_graph = RepoGraph::from_file_tags(&[]);
        let ranked = empty_graph.compute_pagerank(&[]);
        assert!(ranked.is_empty());

        // Single file without definitions or references
        let single_file = FileTags {
            path: PathBuf::from("src/single.rs"),
            definitions: vec![make_tag("foo", 10)],
            references: HashSet::new(),
        };
        let graph = RepoGraph::from_file_tags(&[single_file]);
        let ranked = graph.compute_pagerank(&[]);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].path, PathBuf::from("src/single.rs"));
        assert!((ranked[0].score - 1.0).abs() < 1e-5);
        assert_eq!(ranked[0].ranked_definitions.len(), 1);
        assert_eq!(ranked[0].ranked_definitions[0].tag.name, "foo");
        assert!((ranked[0].ranked_definitions[0].score - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_graph_def_ref_matching() {
        // File A defines Foo, File B references Foo.
        // Edge B -> A makes A have higher PageRank.
        let mut b_refs = HashSet::new();
        b_refs.insert("Foo".to_string());

        let file_a = FileTags {
            path: PathBuf::from("src/a.rs"),
            definitions: vec![make_tag("Foo", 1)],
            references: HashSet::new(),
        };

        let file_b = FileTags {
            path: PathBuf::from("src/b.rs"),
            definitions: Vec::new(),
            references: b_refs,
        };

        let graph = RepoGraph::from_file_tags(&[file_a, file_b]);
        let ranked = graph.compute_pagerank(&[]);

        assert_eq!(ranked.len(), 2);
        // File A should be ranked first because B points to A
        assert_eq!(ranked[0].path, PathBuf::from("src/a.rs"));
        assert_eq!(ranked[1].path, PathBuf::from("src/b.rs"));
        assert!(
            ranked[0].score > ranked[1].score,
            "Expected A score ({}) > B score ({})",
            ranked[0].score,
            ranked[1].score
        );

        // Check definition score in A:
        // Foo was referenced once, so tag_score = file_score * (1.0 + 1.0) = 2.0 * file_score
        let foo_tag = &ranked[0].ranked_definitions[0];
        assert_eq!(foo_tag.tag.name, "Foo");
        let expected_tag_score = ranked[0].score * 2.0;
        assert!(
            (foo_tag.score - expected_tag_score).abs() < 1e-6,
            "Tag score {} should match expected {}",
            foo_tag.score,
            expected_tag_score
        );
    }

    #[test]
    fn test_graph_personalized_pagerank() {
        // With focus on B, directly connected A gets higher score than disconnected C.
        let mut b_refs = HashSet::new();
        b_refs.insert("Foo".to_string());

        let file_a = FileTags {
            path: PathBuf::from("src/a.rs"),
            definitions: vec![make_tag("Foo", 1)],
            references: HashSet::new(),
        };

        let file_b = FileTags {
            path: PathBuf::from("src/b.rs"),
            definitions: Vec::new(),
            references: b_refs,
        };

        let file_c = FileTags {
            path: PathBuf::from("src/c.rs"),
            definitions: vec![make_tag("Bar", 1)],
            references: HashSet::new(),
        };

        let graph = RepoGraph::from_file_tags(&[file_a, file_b, file_c]);
        let focus = vec![PathBuf::from("src/b.rs")];
        let ranked = graph.compute_pagerank(&focus);

        let score_a = ranked
            .iter()
            .find(|f| f.path == Path::new("src/a.rs"))
            .unwrap()
            .score;
        let score_c = ranked
            .iter()
            .find(|f| f.path == Path::new("src/c.rs"))
            .unwrap()
            .score;

        assert!(
            score_a > score_c,
            "Connected node A score ({}) should be higher than disconnected node C score ({})",
            score_a,
            score_c
        );
        // Node C should have 0 score because teleport is 100% on B and no edges lead to C
        assert!(
            score_c < 1e-6,
            "Disconnected node C should have virtually zero score, got {}",
            score_c
        );
    }

    #[test]
    fn test_graph_deterministic() {
        // Running compute_pagerank twice gives identical float scores.
        let mut a_refs = HashSet::new();
        a_refs.insert("Bar".to_string());

        let mut b_refs = HashSet::new();
        b_refs.insert("Foo".to_string());
        b_refs.insert("Baz".to_string());

        let file_a = FileTags {
            path: PathBuf::from("src/a.rs"),
            definitions: vec![make_tag("Foo", 10), make_tag("HelperA", 20)],
            references: a_refs,
        };

        let file_b = FileTags {
            path: PathBuf::from("src/b.rs"),
            definitions: vec![make_tag("Bar", 5)],
            references: b_refs,
        };

        let file_c = FileTags {
            path: PathBuf::from("src/c.rs"),
            definitions: vec![make_tag("Baz", 1)],
            references: HashSet::new(),
        };

        let graph = RepoGraph::from_file_tags(&[file_a, file_b, file_c]);

        let run1 = graph.compute_pagerank(&[]);
        let run2 = graph.compute_pagerank(&[]);

        assert_eq!(run1.len(), run2.len());
        for (f1, f2) in run1.iter().zip(run2.iter()) {
            assert_eq!(f1.path, f2.path);
            assert_eq!(
                f1.score.to_bits(),
                f2.score.to_bits(),
                "Scores must be bit-for-bit identical across runs"
            );
            assert_eq!(f1.ranked_definitions.len(), f2.ranked_definitions.len());
            for (t1, t2) in f1
                .ranked_definitions
                .iter()
                .zip(f2.ranked_definitions.iter())
            {
                assert_eq!(t1.tag.name, t2.tag.name);
                assert_eq!(t1.score.to_bits(), t2.score.to_bits());
            }
        }

        // Also test Personalized PageRank determinism
        let focus = vec![PathBuf::from("src/a.rs")];
        let ppr1 = graph.compute_pagerank(&focus);
        let ppr2 = graph.compute_pagerank(&focus);
        for (f1, f2) in ppr1.iter().zip(ppr2.iter()) {
            assert_eq!(f1.path, f2.path);
            assert_eq!(f1.score.to_bits(), f2.score.to_bits());
        }
    }

    #[test]
    fn test_graph_ppr_fallback_when_focus_not_found() {
        let file_a = FileTags {
            path: PathBuf::from("src/a.rs"),
            definitions: vec![make_tag("Foo", 1)],
            references: HashSet::new(),
        };
        let file_b = FileTags {
            path: PathBuf::from("src/b.rs"),
            definitions: vec![make_tag("Bar", 1)],
            references: HashSet::new(),
        };

        let graph = RepoGraph::from_file_tags(&[file_a, file_b]);
        let uniform = graph.compute_pagerank(&[]);
        let invalid_focus = graph.compute_pagerank(&[PathBuf::from("nonexistent.rs")]);

        assert_eq!(uniform.len(), invalid_focus.len());
        for (u, f) in uniform.iter().zip(invalid_focus.iter()) {
            assert_eq!(u.path, f.path);
            assert_eq!(u.score.to_bits(), f.score.to_bits());
        }
    }

    #[test]
    fn test_graph_symbol_scoring_and_tie_breaking() {
        // File with multiple definitions having different or identical reference counts
        let mut other_refs = HashSet::new();
        other_refs.insert("HotSymbol".to_string());

        let file_defs = FileTags {
            path: PathBuf::from("src/defs.rs"),
            definitions: vec![
                make_tag("TieSecond", 30),
                make_tag("HotSymbol", 50),
                make_tag("TieFirst", 10),
            ],
            references: HashSet::new(),
        };

        let file_caller = FileTags {
            path: PathBuf::from("src/caller.rs"),
            definitions: Vec::new(),
            references: other_refs,
        };

        let graph = RepoGraph::from_file_tags(&[file_defs, file_caller]);
        let ranked = graph.compute_pagerank(&[]);

        let defs_file = ranked
            .iter()
            .find(|f| f.path == Path::new("src/defs.rs"))
            .unwrap();

        assert_eq!(defs_file.ranked_definitions.len(), 3);
        // HotSymbol was referenced 1 time -> score = file_score * 2.0
        assert_eq!(defs_file.ranked_definitions[0].tag.name, "HotSymbol");
        // TieFirst and TieSecond both have 0 references -> score = file_score * 1.0
        // Broken by line ascending: TieFirst (line 10) then TieSecond (line 30)
        assert_eq!(defs_file.ranked_definitions[1].tag.name, "TieFirst");
        assert_eq!(defs_file.ranked_definitions[2].tag.name, "TieSecond");
    }

    #[test]
    fn test_graph_legacy_strategy_matches_existing_behavior() {
        let mut b_refs = HashSet::new();
        b_refs.insert("Foo".to_string());

        let file_a = FileTags {
            path: PathBuf::from("src/a.rs"),
            definitions: vec![make_tag("Foo", 1)],
            references: HashSet::new(),
        };

        let file_b = FileTags {
            path: PathBuf::from("src/b.rs"),
            definitions: Vec::new(),
            references: b_refs,
        };

        let graph = RepoGraph::from_file_tags(&[file_a, file_b]);
        let legacy = graph.compute_ranked_files(&[], RankingStrategy::Legacy);
        let existing = graph.compute_pagerank(&[]);

        assert_eq!(legacy.len(), existing.len());
        for (a, b) in legacy.iter().zip(existing.iter()) {
            assert_eq!(a.path, b.path);
            assert_eq!(a.score.to_bits(), b.score.to_bits());
            assert_eq!(a.ranked_definitions.len(), b.ranked_definitions.len());
        }
    }

    #[test]
    fn test_graph_balanced_strategy_boosts_rare_signal_relative_to_common() {
        let file_common = FileTags {
            path: PathBuf::from("src/common.rs"),
            definitions: vec![make_tag("Common", 1)],
            references: HashSet::new(),
        };
        let file_rare = FileTags {
            path: PathBuf::from("src/rare.rs"),
            definitions: vec![make_tag("Rare", 1)],
            references: HashSet::new(),
        };

        let mut refs_1 = HashSet::new();
        refs_1.insert("Common".to_string());
        refs_1.insert("Rare".to_string());
        let file_ref_1 = FileTags {
            path: PathBuf::from("src/ref1.rs"),
            definitions: Vec::new(),
            references: refs_1,
        };

        let mut refs_2 = HashSet::new();
        refs_2.insert("Common".to_string());
        let file_ref_2 = FileTags {
            path: PathBuf::from("src/ref2.rs"),
            definitions: Vec::new(),
            references: refs_2,
        };

        let mut refs_3 = HashSet::new();
        refs_3.insert("Common".to_string());
        let file_ref_3 = FileTags {
            path: PathBuf::from("src/ref3.rs"),
            definitions: Vec::new(),
            references: refs_3,
        };

        let graph = RepoGraph::from_file_tags(&[
            file_common,
            file_rare,
            file_ref_1,
            file_ref_2,
            file_ref_3,
        ]);
        let legacy = graph.compute_ranked_files(&[], RankingStrategy::Legacy);
        let balanced = graph.compute_ranked_files(&[], RankingStrategy::Balanced);

        let legacy_common = legacy
            .iter()
            .find(|f| f.path == Path::new("src/common.rs"))
            .unwrap()
            .score;
        let legacy_rare = legacy
            .iter()
            .find(|f| f.path == Path::new("src/rare.rs"))
            .unwrap()
            .score;
        let balanced_common = balanced
            .iter()
            .find(|f| f.path == Path::new("src/common.rs"))
            .unwrap()
            .score;
        let balanced_rare = balanced
            .iter()
            .find(|f| f.path == Path::new("src/rare.rs"))
            .unwrap()
            .score;

        let legacy_ratio = legacy_rare / legacy_common;
        let balanced_ratio = balanced_rare / balanced_common;
        assert!(
            balanced_ratio > legacy_ratio,
            "Balanced ratio ({balanced_ratio}) should be greater than legacy ratio ({legacy_ratio})"
        );
    }
}
