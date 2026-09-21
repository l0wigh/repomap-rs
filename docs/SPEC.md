# SPEC — repomap-rs — v1.1 — 2026-09-21

## 1. Context and objective
repomap-rs is a Rust-based CLI and library designed to generate a concise, high-value repository map ("repomap") inspired by Aider. Its primary objective is to provide autonomous AI coding agents with a structured overview of a codebase (file paths, classes, functions, signatures) that prioritizes the most relevant context and fits strictly within a user-defined LLM token budget.

## 2. Scope: explicitly in / explicitly out
### Explicitly In:
- Local Git repository scanning with strict `.gitignore` and hidden file handling via the `ignore` crate.
- Abstract Syntax Tree (AST) parsing for definitions (types, functions, methods, classes) and references for 7 core languages: Rust, Python, JavaScript, TypeScript, Go, C, and C++.
- Dependency / reference graph construction between files and symbols.
- PageRank and Personalized PageRank (PPR) algorithms to score importance, with optional target "focus" files.
- Accurate token counting and budget trimming using `tiktoken-rs` (OpenAI tokenizers: `cl100k_base`, `o200k_base`).
- Two output formats: Aider-style hierarchical indented text (default) and structured JSON.
- Comprehensive Test-Driven Development (TDD) coverage: unit tests, mock fixtures, and integration tests.

### Explicitly Out:
- Direct LLM calls or network API interaction (purely local offline execution).
- Dynamic analysis, runtime tracing, or semantic LSP-based indexing (type resolution/jump to definition across packages).
- Interactive GUI or TUI.
- Modifying or editing repository code.

## 3. Users and use cases
- **Primary User**: AI orchestrator / coding agent CLI pipelines and developers using AI assistants.
- **Use Case 1 (Global Context)**: Agent launches `repomap-rs` at repository root to obtain a top-level map under 1024 tokens.
- **Use Case 2 (Task-Focused Context)**: Agent passes `--focus src/auth.rs` to compute a Personalized PageRank biased towards files related to authentication.
- **Use Case 3 (Structured Tool Integration)**: Agent or IDE plugin invokes `repomap-rs --format json` to ingest structured symbol scores and files.

## 4. Functional requirements
- **FR-01: File Discovery & Filtering**
  - Priority: Must
  - Description: Recursively discover source files from target directory, strictly honoring `.gitignore`, `.git` directory exclusions, binary file detection, and non-supported file extensions.
  - Acceptance: Running discovery on a Git repo ignores ignored files, `.git/`, and binary files. Command: `cargo test test_scanner_` passes.
- **FR-02: Multi-Language AST Symbol & Reference Extraction**
  - Priority: Must
  - Description: Parse files using `tree-sitter` to extract definitions (classes, structs, interfaces, functions, methods, traits, namespaces) with line numbers and signature text, as well as identifier references.
  - Supported grammars in V1.1: Rust, Python, JavaScript, TypeScript, Go, C, C++.
  - Acceptance: For each supported language fixture, all defined top-level/member symbols and referenced identifiers are accurately extracted. Command: `cargo test test_parser_` passes.
- **FR-03: Reference Graph & (Personalized) PageRank Scoring**
  - Priority: Must
  - Description: Build a reference graph linking files where file A references symbols defined in file B. Compute PageRank scores. If `--focus <paths>` is provided, compute Personalized PageRank (PPR) with teleport distribution concentrated on focus files.
  - Acceptance: Graph accurately reflects known references between test fixture files; focus file elevates ranking of directly and indirectly connected files. Command: `cargo test test_graph_` passes.
- **FR-04: Strict Token Budgeting & Selection**
  - Priority: Must
  - Description: Pack file paths and symbol definitions into the final output using a greedy/priority algorithm based on PageRank scores, ensuring total token count does not exceed `--max-tokens`. Token count measured using `tiktoken-rs`.
  - Acceptance: Output measured by `tiktoken-rs` has token count $\le$ `max_tokens`. Command: `cargo test test_budget_` passes.
- **FR-05: Output Formatting**
  - Priority: Must
  - Description: Support two formats:
    1. `aider` (default): Concise indented tree displaying file paths and symbol signatures.
    2. `json`: JSON output containing list of files, selected symbols, ranks, and exact token count.
  - Acceptance: Both formatters produce valid output conforming to schemas on fixtures. Command: `cargo test test_formatter_` passes.
- **FR-06: CLI Interface**
  - Priority: Must
  - Description: CLI options via `clap`:
    - `[PATH]`: Target directory (default: current working directory `.`).
    - `--focus <FILE>...`: One or more paths to bias ranking.
    - `--max-tokens <N>`: Maximum token budget (default: 1024).
    - `--format <aider|json>`: Output format (default: `aider`).
    - `--encoding <cl100k_base|o200k_base>`: Tokenizer encoding (default: `cl100k_base`).
  - Acceptance: CLI executes successfully and returns exit code 0 on valid arguments; displays helpful errors and non-zero exit code on invalid arguments. Command: `cargo test test_cli_` passes.

## 5. Non-functional requirements
- **NFR-01 (Performance)**: Map generation for a repository with 1,000 source files completes in $< 500$ ms in release mode.
- **NFR-02 (Memory Consumption)**: Peak resident memory (RSS) stays under 200 MB on a 5,000 file repository.
- **NFR-03 (Robustness)**: Syntax errors, malformed source code, or unparseable files must never crash the tool; unparseable files are skipped or partially parsed with tree-sitter error recovery.
- **NFR-04 (Determinism)**: Given identical arguments and repository state, the output is strictly deterministic (reproducible ordering).

## 6. Technical constraints and environment
- **Language**: Rust (edition 2024, rustc 1.96.1).
- **Target OS**: Linux x86_64 / POSIX.
- **Dependencies**:
  - `clap`: CLI argument parsing (v4, features: `derive`).
  - `tree-sitter`: AST parsing (v0.27 or compatible) + official tree-sitter language grammars (`tree-sitter-rust`, `tree-sitter-python`, `tree-sitter-javascript`, `tree-sitter-typescript`, `tree-sitter-go`, `tree-sitter-c`, `tree-sitter-cpp`).
  - `ignore`: File walker respecting `.gitignore`.
  - `tiktoken-rs`: Accurate BPE tokenization.
  - `serde`, `serde_json`: Serialization for JSON output.
  - `thiserror`, `anyhow`: Error management.

## 7. Target architecture and file tree
```
repomap-rs/
├── Cargo.toml
├── docs/
│   ├── SPEC.md
│   ├── PLAN.md
│   └── JOURNAL.md
├── src/
│   ├── lib.rs              # Library exports
│   ├── main.rs             # CLI binary entrypoint
│   ├── scanner.rs          # File discovery and gitignore filtering
│   ├── parser/             # AST parsing & symbol extraction
│   │   ├── mod.rs          # Tag, Symbol, and LanguageParser trait
│   │   ├── rust.rs
│   │   ├── python.rs
│   │   ├── javascript.rs
│   │   ├── typescript.rs
│   │   └── go.rs
│   ├── graph.rs            # Dependency graph, PageRank & PPR
│   ├── budget.rs           # Token budget & greedy symbol selector
│   └── formatter/          # Output formatting
│       ├── mod.rs
│       ├── aider.rs
│       └── json.rs
└── tests/
    ├── fixtures/           # Multi-language sample repositories
    └── integration_test.rs # End-to-end integration tests
```

## 8. Data and integrations
- **Input Data**: Local filesystem files within specified repository root.
- **External Integrations**: None (no network, no remote APIs).

## 9. Test strategy and Definition of Done
- **Test-Driven Development (TDD)**:
  - Phase 1: Define unit test cases for scanner, each language parser, graph PageRank, token budgeting, and formatting before/alongside logic.
  - Phase 2: Create mock multi-language repo fixture in `tests/fixtures`.
  - Phase 3: Integration tests validating CLI flags and output formats.
- **Definition of Done (DoD)**:
  - 100% of functional requirements (FR-01 to FR-06) verified by automated tests.
  - `cargo test` passes with zero failures.
  - `cargo clippy --all-targets -- -D warnings` passes with zero warnings.
  - `cargo fmt --check` passes.
  - Deterministic output demonstrated on fixture repositories.

## 10. Deliverables
- Fully functional `repomap-rs` Rust library and CLI binary.
- Complete test suite in `tests/` and unit tests in `src/`.
- Documentation: `docs/SPEC.md`, `docs/PLAN.md`, `docs/JOURNAL.md`, `README.md`.

## 11. Validated decisions and assumptions (with date)
- 2026-09-21: Multi-language support in V1 (Rust, Python, JS, TS, Go) via `tree-sitter`.
- 2026-09-21: Personalized PageRank (PPR) with `--focus` support + global PageRank fallback.
- 2026-09-21: Real token counting via `tiktoken-rs` (`cl100k_base` default).
- 2026-09-21: Dual output formats: Aider-style text (default) and JSON.
- 2026-09-21: Git file discovery via `ignore` crate.
- 2026-09-21: Scope amendment v1.1 adding C and C++ grammars and extensions.

## 12. Risks
- **Grammar build complexity**: C bindings required for certain tree-sitter grammars (e.g. `cc` build dependency). Mitigation: Use standard `tree-sitter-*` crates which bundle C sources with `cc` in `build.rs`.
- **Token budget boundary edge cases**: A single file signature might exceed remaining budget. Mitigation: Budget selector greedily includes only symbols that fit, omitting whole files or remaining symbols once budget is saturated.

## 13. Open points
*(None — all points resolved)*

## 14. Out of scope / future evolutions
- Additional language grammars (Java, C#, Ruby, PHP).
- LSP semantic cross-file resolution.
- Git diff/commit-based incremental caching.

## 15. Version history
- v1.0 (2026-09-21): Initial specification approved for review.
- v1.1 (2026-09-21): Scope amendment adding C and C++ language AST support.
