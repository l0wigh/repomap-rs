use std::collections::HashSet;
use std::path::Path;

use super::LanguageParser;
use crate::{FileTags, RepomapError, Result, SymbolKind, Tag};

pub struct PythonParser {
    language: tree_sitter::Language,
}

impl PythonParser {
    pub fn new() -> Result<Self> {
        let language: tree_sitter::Language = tree_sitter_python::LANGUAGE.into();
        Ok(Self { language })
    }
}

impl Default for PythonParser {
    fn default() -> Self {
        Self::new().expect("Failed to initialize PythonParser")
    }
}

impl LanguageParser for PythonParser {
    fn parse(&self, path: &Path, content: &str) -> Result<FileTags> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&self.language)
            .map_err(|e| RepomapError::Parser(e.to_string()))?;

        let tree = parser.parse(content, None).ok_or_else(|| {
            RepomapError::Parser("Failed to parse Python source code".to_string())
        })?;

        let mut definitions = Vec::new();
        let mut references = HashSet::new();
        let mut def_name_ids = HashSet::new();
        let mut def_names = HashSet::new();

        traverse(
            tree.root_node(),
            content,
            &mut definitions,
            &mut references,
            &mut def_name_ids,
            &mut def_names,
        );

        references.retain(|r| !def_names.contains(r));

        Ok(FileTags {
            path: path.to_path_buf(),
            definitions,
            references,
        })
    }
}

fn is_method(node: &tree_sitter::Node) -> bool {
    let mut curr = node.parent();
    while let Some(parent) = curr {
        match parent.kind() {
            "class_definition" => return true,
            _ => curr = parent.parent(),
        }
    }
    false
}

fn extract_signature(node: &tree_sitter::Node, content: &str) -> String {
    let text = &content[node.byte_range()];
    let first_line = text.lines().next().unwrap_or("").trim();
    if let Some(idx) = first_line.rfind(':') {
        first_line[..=idx].trim().to_string()
    } else {
        first_line.to_string()
    }
}

fn traverse(
    node: tree_sitter::Node,
    content: &str,
    definitions: &mut Vec<Tag>,
    references: &mut HashSet<String>,
    def_name_ids: &mut HashSet<usize>,
    def_names: &mut HashSet<String>,
) {
    let kind = match node.kind() {
        "class_definition" => Some(SymbolKind::Class),
        "function_definition" => {
            if is_method(&node) {
                Some(SymbolKind::Method)
            } else {
                Some(SymbolKind::Function)
            }
        }
        _ => None,
    };

    if let Some(symbol_kind) = kind
        && let Some(name_node) = node.child_by_field_name("name")
        && let Ok(name_str) = name_node.utf8_text(content.as_bytes())
        && !name_str.is_empty()
    {
        def_name_ids.insert(name_node.id());
        def_names.insert(name_str.to_string());
        let signature = extract_signature(&node, content);
        let line = node.start_position().row + 1;
        definitions.push(Tag {
            name: name_str.to_string(),
            kind: symbol_kind,
            line,
            signature,
        });
    }

    if node.kind() == "identifier"
        && !def_name_ids.contains(&node.id())
        && let Ok(ident_text) = node.utf8_text(content.as_bytes())
        && !ident_text.is_empty()
    {
        references.insert(ident_text.to_string());
    }

    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i) {
            traverse(
                child,
                content,
                definitions,
                references,
                def_name_ids,
                def_names,
            );
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn test_parse_python_classes_and_functions() {
        let parser = PythonParser::new().unwrap();
        let code = r#"
class BaseUser:
    pass

class User(BaseUser):
    pass

def create_user(name: str) -> User:
    return User()
"#;
        let tags = parser.parse(Path::new("user.py"), code).unwrap();
        assert_eq!(tags.path, Path::new("user.py"));

        let base_user = tags
            .definitions
            .iter()
            .find(|t| t.name == "BaseUser")
            .expect("BaseUser missing");
        assert_eq!(base_user.kind, SymbolKind::Class);
        assert_eq!(base_user.line, 2);
        assert_eq!(base_user.signature, "class BaseUser:");

        let user = tags
            .definitions
            .iter()
            .find(|t| t.name == "User")
            .expect("User missing");
        assert_eq!(user.kind, SymbolKind::Class);
        assert_eq!(user.line, 5);
        assert_eq!(user.signature, "class User(BaseUser):");

        let create_user = tags
            .definitions
            .iter()
            .find(|t| t.name == "create_user")
            .expect("create_user missing");
        assert_eq!(create_user.kind, SymbolKind::Function);
        assert_eq!(create_user.line, 8);
        assert_eq!(create_user.signature, "def create_user(name: str) -> User:");
    }

    #[test]
    fn test_parse_python_methods_and_async() {
        let parser = PythonParser::new().unwrap();
        let code = r#"
class Account:
    def __init__(self, balance: int):
        self.balance = balance

    async def transfer(self, target: str, amount: int) -> bool:
        return True

async def fetch_data(url: str) -> bytes:
    return b""
"#;
        let tags = parser.parse(Path::new("account.py"), code).unwrap();

        let account = tags
            .definitions
            .iter()
            .find(|t| t.name == "Account")
            .expect("Account missing");
        assert_eq!(account.kind, SymbolKind::Class);
        assert_eq!(account.line, 2);
        assert_eq!(account.signature, "class Account:");

        let init_method = tags
            .definitions
            .iter()
            .find(|t| t.name == "__init__")
            .expect("__init__ missing");
        assert_eq!(init_method.kind, SymbolKind::Method);
        assert_eq!(init_method.line, 3);
        assert_eq!(init_method.signature, "def __init__(self, balance: int):");

        let transfer_method = tags
            .definitions
            .iter()
            .find(|t| t.name == "transfer")
            .expect("transfer missing");
        assert_eq!(transfer_method.kind, SymbolKind::Method);
        assert_eq!(transfer_method.line, 6);
        assert_eq!(
            transfer_method.signature,
            "async def transfer(self, target: str, amount: int) -> bool:"
        );

        let fetch_data_fn = tags
            .definitions
            .iter()
            .find(|t| t.name == "fetch_data")
            .expect("fetch_data missing");
        assert_eq!(fetch_data_fn.kind, SymbolKind::Function);
        assert_eq!(fetch_data_fn.line, 9);
        assert_eq!(
            fetch_data_fn.signature,
            "async def fetch_data(url: str) -> bytes:"
        );
    }

    #[test]
    fn test_parse_python_references() {
        let parser = PythonParser::new().unwrap();
        let code = r#"
import os
from math import sqrt

def calculate(value):
    temp = sqrt(value)
    return os.path.join(str(temp))
"#;
        let tags = parser.parse(Path::new("calc.py"), code).unwrap();

        assert_eq!(tags.definitions.len(), 1);
        assert_eq!(tags.definitions[0].name, "calculate");

        // Definitions must not be in references
        assert!(!tags.references.contains("calculate"));

        // Imported and used identifiers
        assert!(tags.references.contains("os"));
        assert!(tags.references.contains("math"));
        assert!(tags.references.contains("sqrt"));
        assert!(tags.references.contains("value"));
        assert!(tags.references.contains("temp"));
        assert!(tags.references.contains("join"));
        assert!(tags.references.contains("str"));
    }

    #[test]
    fn test_parse_python_malformed() {
        let parser = PythonParser::new().unwrap();
        let code = r#"
class Incomplete:
    def 

def valid_function(x):
    return x * 2

def syntax_error(
"#;
        let tags = parser.parse(Path::new("broken.py"), code).unwrap();

        let valid_fn = tags.definitions.iter().find(|t| t.name == "valid_function");
        assert!(valid_fn.is_some());
        assert_eq!(valid_fn.unwrap().kind, SymbolKind::Function);
    }
}
