use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Tag {
    pub name: String,
    pub kind: SymbolKind,
    pub line: usize,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTags {
    pub path: PathBuf,
    pub definitions: Vec<Tag>,
    pub references: HashSet<String>,
}

#[derive(thiserror::Error, Debug)]
pub enum RepomapError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Parsing error: {0}")]
    Parser(String),
    #[error("Graph error: {0}")]
    Graph(String),
    #[error("Tokenizer error: {0}")]
    Tokenizer(String),
    #[error("Error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, RepomapError>;

pub mod budget;
pub mod formatter;
pub mod graph;
pub mod parser;
pub mod scanner;
