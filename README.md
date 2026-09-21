# repomap-rs

> [!NOTE]
> **100% AI Coded**: `repomap-rs` was built entirely through agentic pair programming—orchestrated by AI agents, guided by rigorous Test-Driven Development (TDD), automated specifications, strict modular contracts, and zero human manual code editing.

A fast, lightweight Rust CLI and library for generating concise, high-value repository maps ("repomap") tailored for LLM code assistants and autonomous AI coding agents. Inspired by Aider's repository mapping architecture.

`repomap-rs` analyzes your codebase's Abstract Syntax Trees (AST), extracts symbol definitions and cross-file references, builds a dependency graph, ranks files and symbols using PageRank (or Personalized PageRank when focused on specific files), and fits the resulting map into a strict token budget measured with `tiktoken`.

---

## Features

- **Multi-Language AST Parsing**: Full symbol definition (functions, methods, classes, structs, traits, interfaces, enums, type aliases) and reference extraction using Tree-sitter for:
  - **Rust** (`.rs`)
  - **Python** (`.py`)
  - **JavaScript** (`.js`, `.jsx`, `.mjs`, `.cjs`)
  - **TypeScript** (`.ts`, `.tsx`, `.mts`, `.cts`)
  - **Go** (`.go`)
  - **C** (`.c`, `.h`)
  - **C++** (`.cpp`, `.cc`, `.cxx`, `.hpp`, `.hh`, `.hxx`)
  - **PHP** (`.php`, `.phtml`)
  - **Java** (`.java`)
  - **C#** (`.cs`)
  - **Ruby** (`.rb`)
  - **Kotlin** (`.kt`, `.kts`)
- **Gitignore & Hidden File Filtering**: Native directory traversal respecting `.gitignore`, `.git/` exclusions, and hidden files via the `ignore` crate.
- **Reference Graph & PageRank**: Automatically infers directed dependency edges between referencing files and defining files. Computes global PageRank or Personalized PageRank (PPR) when biased towards `--focus` targets.
- **Strict Token Budgeting**: Accurate BPE token counting with `tiktoken-rs` (`cl100k_base` and `o200k_base`). A greedy selection algorithm ensures the generated map never exceeds the configured token limit.
- **Dual Output Formats**:
  - **`aider`** (default): Human- and LLM-friendly indented outline (`│⋮...` blocks).
  - **`json`**: Structured JSON output for programmatic tool calling and indexing.
- **Deterministic & Robust**: Identical runs produce bit-for-bit identical outputs. Malformed files and syntax errors are gracefully handled via AST error recovery.

---

## Installation

### From Source

Ensure you have Rust and Cargo installed (edition 2024 or recent stable Rust toolchain):

```bash
cargo build --release
```

The compiled binary will be located at `target/release/repomap`.

---

## CLI Usage

```text
Repository map generator for LLM code assistants

Usage: repomap [OPTIONS] [ROOT]

Arguments:
  [ROOT]  Target directory [default: .]

Options:
      --focus <FILE>...    One or more paths to bias ranking
      --max-tokens <N>     Maximum token budget [default: 1024]
      --format <FORMAT>    Output format [default: aider] [possible values: aider, json]
      --encoding <ENCODING> Tokenizer encoding [default: cl100k_base] [possible values: cl100k_base, o200k_base]
  -h, --help               Print help
```

### Examples

#### 1. Generate a default repository map under 1024 tokens
```bash
repomap .
```

Sample Aider output:
```text
src/server.go:
│⋮...
│type ServerHandler struct
│⋮...
│func NewServerHandler(c AppController) *ServerHandler

src/app.ts:
│⋮...
│export class AppController

src/service.rs:
│⋮...
│pub struct UserService
```

#### 2. Focus on specific files (Personalized PageRank)
Elevates the relevance of the target file and any connected files referencing or referenced by it:
```bash
repomap . --focus src/auth.py
```

#### 3. Output as JSON under a custom token budget
```bash
repomap . --max-tokens 500 --format json --encoding o200k_base
```

Sample JSON output:
```json
{
  "files": [
    {
      "path": "src/auth.py",
      "score": 0.85,
      "definitions": [
        {
          "name": "AuthClient",
          "kind": "Class",
          "line": 1,
          "signature": "class AuthClient:",
          "score": 1.7
        }
      ]
    }
  ],
  "total_tokens": 42,
  "max_tokens": 500
}
```

---

## Library Usage

Add `repomap-rs` to your `Cargo.toml`:

```toml
[dependencies]
repomap-rs = { path = "/path/to/repomap-rs" }
```

Use the pipeline in your Rust application:

```rust
use std::path::Path;
use repomap_rs::{scanner, parser, graph::RepoGraph, budget, formatter::{self, OutputFormat}};

fn main() -> repomap_rs::Result<()> {
    let root = Path::new(".");

    // 1. Discover files
    let files = scanner::scan_repository(root)?;

    // 2. Parse symbols and references
    let mut file_tags = Vec::new();
    for file in files {
        let full_path = root.join(&file);
        if let Ok(content) = std::fs::read_to_string(&full_path) {
            if let Some(tags) = parser::parse_file(&file, &content)? {
                file_tags.push(tags);
            }
        }
    }

    // 3. Construct reference graph & compute PageRank
    let graph = RepoGraph::from_file_tags(&file_tags);
    let ranked = graph.compute_pagerank(&[]); // Or pass focus paths: &[PathBuf::from("src/main.rs")]

    // 4. Fit to token budget
    let budget_plan = budget::fit_to_budget(&ranked, 1024, "cl100k_base")?;

    // 5. Render output
    let output = formatter::format_repomap(&budget_plan, OutputFormat::Aider)?;
    println!("{}", output);

    Ok(())
}
```

---

## Testing

Run unit tests:
```bash
cargo test --lib
```

Run integration tests:
```bash
cargo test --test integration_test
```

Run linter and formatting checks:
```bash
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

---

## Development & Methodology

`repomap-rs` was 100% **AI coded**:
- **Agentic Orchestration**: System architecture, task decomposition, and code generation were autonomously executed by AI agents in pair-programming collaboration.
- **Strict Test-Driven Development (TDD)**: Every component and feature was built against comprehensive unit and integration test suites, ensuring correctness, determinism, and robust AST error handling.
- **Modular Contracts & Verification**: Zero human manual code writing—all implementation, refactoring, and documentation were delivered by agents guided by formal specifications, strict modular contracts, and automated validation (`cargo test`, `cargo clippy`, `cargo fmt`).

