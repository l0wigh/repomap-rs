# PLAN — repomap-rs

Execution plan for repomap-rs implementation following TDD methodology and strict component boundaries.

## Architecture and Contracts Overview

### Shared Data Types (`src/lib.rs` / `src/parser/mod.rs`):
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Trait,
    Interface,
    TypeAlias,
    Enum,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    pub kind: SymbolKind,
    pub line: usize,
    pub signature: String,
}

#[derive(Debug, Clone)]
pub struct FileTags {
    pub path: PathBuf,
    pub definitions: Vec<Tag>,
    pub references: HashSet<String>,
}
```

---

## Waves and Tasks

### Wave 1 — Foundation: Dependencies & File Scanner

#### T-01 | Project Skeleton & Dependencies Setup [STATUS: DONE]
- **Covers**: Technical Constraints
- **Depends on**: — | **Wave**: 1
- **Write**: `Cargo.toml`, `src/lib.rs`
- **Read**: `docs/SPEC.md`
- **Forbidden**: `src/scanner.rs`, `src/parser/*`, `src/graph.rs`, `src/budget.rs`, `src/formatter/*`, `src/main.rs`
- **Toolsets**: terminal, file
- **Acceptance**: `cargo check` → Finished dev profile with 0 errors.

#### T-02 | File Discovery Scanner with Gitignore Support (TDD) [STATUS: DONE]
- **Covers**: FR-01
- **Depends on**: T-01 | **Wave**: 1 (sequential after T-01)
- **Write**: `src/scanner.rs`
- **Read**: `src/lib.rs`, `Cargo.toml`
- **Forbidden**: `src/parser/*`, `src/graph.rs`, `src/budget.rs`, `src/formatter/*`, `src/main.rs`
- **Toolsets**: terminal, file
- **Acceptance**: `cargo test scanner::tests` → 100% pass (validates gitignore honoring, hidden file rejection, supported extensions filter).

---

### Wave 2 — Symbol & Reference AST Parsers (TDD)

#### T-03 | Parser Abstraction & Rust AST Parser [STATUS: DONE]
- **Covers**: FR-02
- **Depends on**: T-01 | **Wave**: 2
- **Write**: `src/parser/mod.rs`, `src/parser/rust.rs`
- **Read**: `src/lib.rs`, `Cargo.toml`
- **Forbidden**: `src/parser/python.rs`, `src/parser/javascript.rs`, `src/parser/typescript.rs`, `src/parser/go.rs`, `src/scanner.rs`, `src/graph.rs`, `src/budget.rs`, `src/formatter/*`, `src/main.rs`
- **Toolsets**: terminal, file
- **Acceptance**: `cargo test parser::rust::tests` → 100% pass (extracts structs, enums, traits, functions, methods, and identifier references).

#### T-04 | Python AST Parser [STATUS: DONE]
- **Covers**: FR-02
- **Depends on**: T-01 | **Wave**: 2
- **Write**: `src/parser/python.rs`
- **Read**: `src/parser/mod.rs`, `Cargo.toml`
- **Forbidden**: `src/parser/rust.rs`, `src/parser/javascript.rs`, `src/parser/typescript.rs`, `src/parser/go.rs`, `src/scanner.rs`, `src/graph.rs`, `src/budget.rs`, `src/formatter/*`, `src/main.rs`
- **Toolsets**: terminal, file
- **Acceptance**: `cargo test parser::python::tests` → 100% pass (extracts classes, async/def functions, methods, signatures, and references).

#### T-05 | JavaScript & TypeScript AST Parsers [STATUS: DONE]
- **Covers**: FR-02
- **Depends on**: T-01 | **Wave**: 2
- **Write**: `src/parser/javascript.rs`, `src/parser/typescript.rs`
- **Read**: `src/parser/mod.rs`, `Cargo.toml`
- **Forbidden**: `src/parser/rust.rs`, `src/parser/python.rs`, `src/parser/go.rs`, `src/scanner.rs`, `src/graph.rs`, `src/budget.rs`, `src/formatter/*`, `src/main.rs`
- **Toolsets**: terminal, file
- **Acceptance**: `cargo test parser::javascript::tests && cargo test parser::typescript::tests` → 100% pass (extracts classes, interfaces, types, functions, arrow functions, methods).

#### T-06 | Go AST Parser [STATUS: DONE]
- **Covers**: FR-02
- **Depends on**: T-01 | **Wave**: 2
- **Write**: `src/parser/go.rs`
- **Read**: `src/parser/mod.rs`, `Cargo.toml`
- **Forbidden**: `src/parser/rust.rs`, `src/parser/python.rs`, `src/parser/javascript.rs`, `src/parser/typescript.rs`, `src/scanner.rs`, `src/graph.rs`, `src/budget.rs`, `src/formatter/*`, `src/main.rs`
- **Toolsets**: terminal, file
- **Acceptance**: `cargo test parser::go::tests` → 100% pass (extracts structs, interfaces, functions, receiver methods, and references).

---

### Wave 3 — Reference Graph & Personalized PageRank

#### T-07 | Dependency Graph & (Personalized) PageRank Engine (TDD) [STATUS: DONE]
- **Covers**: FR-03
- **Depends on**: T-03 | **Wave**: 3
- **Write**: `src/graph.rs`
- **Read**: `src/parser/mod.rs`, `src/lib.rs`
- **Forbidden**: `src/scanner.rs`, `src/parser/*`, `src/budget.rs`, `src/formatter/*`, `src/main.rs`
- **Toolsets**: terminal, file
- **Acceptance**: `cargo test graph::tests` → 100% pass (validates graph creation, def/ref edge matching, convergence of PageRank, and biased scoring with focus files).

---

### Wave 4 — Token Budgeting & Formatters

#### T-08 | Token Budgeting & Symbol Selection Engine (TDD) [STATUS: DONE]
- **Covers**: FR-04
- **Depends on**: T-07 | **Wave**: 4
- **Write**: `src/budget.rs`
- **Read**: `src/parser/mod.rs`, `src/graph.rs`, `src/lib.rs`
- **Forbidden**: `src/scanner.rs`, `src/parser/*`, `src/formatter/*`, `src/main.rs`
- **Toolsets**: terminal, file
- **Acceptance**: `cargo test budget::tests` → 100% pass (validates tiktoken counting, greedy packing of symbols, strict <= max_tokens guarantee).

#### T-09 | Output Formatters: Aider Tree & Structured JSON (TDD) [STATUS: DONE]
- **Covers**: FR-05
- **Depends on**: T-08 | **Wave**: 4
- **Write**: `src/formatter/mod.rs`, `src/formatter/aider.rs`, `src/formatter/json.rs`
- **Read**: `src/budget.rs`, `src/parser/mod.rs`, `src/lib.rs`
- **Forbidden**: `src/scanner.rs`, `src/parser/*`, `src/graph.rs`, `src/main.rs`
- **Toolsets**: terminal, file
- **Acceptance**: `cargo test formatter::tests` → 100% pass (verifies Aider-style indentation and JSON schema compliance).

---

### Wave 5 — CLI & End-to-End Integration

#### T-10 | CLI Application Entrypoint [STATUS: DONE]
- **Covers**: FR-06
- **Depends on**: T-02, T-07, T-08, T-09 | **Wave**: 5
- **Write**: `src/main.rs`
- **Read**: `src/lib.rs`, `docs/SPEC.md`
- **Forbidden**: modifying `src/scanner.rs`, `src/parser/*`, `src/graph.rs`, `src/budget.rs`, `src/formatter/*`
- **Toolsets**: terminal, file
- **Acceptance**: `cargo run -- --help` → Displays CLI options; exit code 0.

#### T-11 | Integration Test Suite & Multi-Language Fixtures [STATUS: DONE]
- **Covers**: FR-01, FR-02, FR-03, FR-04, FR-05, FR-06, NFR-01..04
- **Depends on**: T-10 | **Wave**: 5
- **Write**: `tests/fixtures/...`, `tests/integration_test.rs`, `README.md`
- **Read**: `src/*`, `docs/SPEC.md`
- **Forbidden**: —
- **Toolsets**: terminal, file
- **Acceptance**: `cargo test --test integration_test` → 100% pass; `cargo clippy --all-targets -- -D warnings` → 0 warnings; `cargo fmt --check` → OK.

---

### Wave 6 — Scope Amendment v1.1: C & C++ AST Support

#### T-12 | Dependencies & Scanner Extension for C & C++ (TDD) [STATUS: DONE]
- **Covers**: FR-01, FR-02
- **Depends on**: T-11 | **Wave**: 6
- **Write**: `Cargo.toml`, `src/scanner.rs`
- **Read**: `docs/SPEC.md`
- **Forbidden**: `src/parser/*`, `src/graph.rs`, `src/budget.rs`, `src/formatter/*`, `src/main.rs`
- **Toolsets**: terminal, file
- **Acceptance**: `cargo test scanner::tests` → 100% pass (validates detection of `.c`, `.h`, `.cpp`, `.cc`, `.cxx`, `.hpp`, `.hh`, `.hxx`).

#### T-13 | C & C++ AST Parsers (TDD) [STATUS: DONE]
- **Covers**: FR-02
- **Depends on**: T-12 | **Wave**: 6
- **Write**: `src/parser/c.rs`, `src/parser/cpp.rs`, `src/parser/mod.rs`
- **Read**: `src/parser/rust.rs`, `src/lib.rs`, `Cargo.toml`
- **Forbidden**: `src/scanner.rs`, `src/graph.rs`, `src/budget.rs`, `src/formatter/*`, `src/main.rs`
- **Toolsets**: terminal, file
- **Acceptance**: `cargo test parser::c::tests && cargo test parser::cpp::tests` → 100% pass (extracts functions, structs, classes, namespaces, methods, typedefs, and references).

#### T-14 | Integration Test Suite & Fixtures Update for C & C++ [STATUS: DONE]
- **Covers**: FR-01 to FR-06, NFR-01..04
- **Depends on**: T-13 | **Wave**: 6
- **Write**: `tests/fixtures/sample_repo/module.c`, `tests/fixtures/sample_repo/engine.cpp`, `tests/integration_test.rs`, `README.md`
- **Read**: `src/*`, `docs/SPEC.md`
- **Forbidden**: `src/scanner.rs`, `src/parser/*`, `src/graph.rs`, `src/budget.rs`, `src/formatter/*`, `src/main.rs`
- **Toolsets**: terminal, file
- **Acceptance**: `cargo test --test integration_test` → 100% pass; `cargo clippy --all-targets -- -D warnings` → 0 warnings; `cargo fmt --check` → OK.

---

### Wave 7 — Scope Amendment v1.2: Web & API Majors (PHP, Java, C#, Ruby, Kotlin)

#### T-15 | Dependencies & Scanner Extension for PHP, Java, C#, Ruby, Kotlin (TDD) [STATUS: DONE]
- **Covers**: FR-01, FR-02
- **Depends on**: T-14 | **Wave**: 7
- **Write**: `Cargo.toml`, `src/scanner.rs`
- **Read**: `docs/SPEC.md`
- **Forbidden**: `src/parser/*`, `src/graph.rs`, `src/budget.rs`, `src/formatter/*`, `src/main.rs`
- **Toolsets**: terminal, file
- **Acceptance**: `cargo test scanner::tests` → 100% pass (validates detection of `.php`, `.phtml`, `.java`, `.cs`, `.rb`, `.kt`, `.kts`).

#### T-16 | AST Parsers for PHP, Java, C#, Ruby, Kotlin (TDD) [STATUS: DONE]
- **Covers**: FR-02
- **Depends on**: T-15 | **Wave**: 7
- **Write**: `src/parser/php.rs`, `src/parser/java.rs`, `src/parser/csharp.rs`, `src/parser/ruby.rs`, `src/parser/kotlin.rs`, `src/parser/mod.rs`
- **Read**: `src/parser/rust.rs`, `src/lib.rs`, `Cargo.toml`
- **Forbidden**: `src/scanner.rs`, `src/graph.rs`, `src/budget.rs`, `src/formatter/*`, `src/main.rs`
- **Toolsets**: terminal, file
- **Acceptance**: `cargo test parser::php::tests && cargo test parser::java::tests && cargo test parser::csharp::tests && cargo test parser::ruby::tests && cargo test parser::kotlin::tests` → 100% pass.

#### T-17 | Multi-Language Fixtures, Integration Tests & Docs Update [STATUS: DONE]
- **Covers**: FR-01 to FR-06, NFR-01..04
- **Depends on**: T-16 | **Wave**: 7
- **Write**: `tests/fixtures/sample_repo/...`, `tests/integration_test.rs`, `README.md`
- **Read**: `src/*`, `docs/SPEC.md`
- **Forbidden**: `src/scanner.rs`, `src/parser/*`, `src/graph.rs`, `src/budget.rs`, `src/formatter/*`, `src/main.rs`
- **Toolsets**: terminal, file
- **Acceptance**: `cargo test --test integration_test` → 100% pass; `cargo clippy --all-targets -- -D warnings` → 0 warnings; `cargo fmt --check` → OK.
