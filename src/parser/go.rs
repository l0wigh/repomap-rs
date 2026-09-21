use std::collections::HashSet;
use std::path::Path;

use super::LanguageParser;
use crate::{FileTags, RepomapError, Result, SymbolKind, Tag};

pub struct GoParser {
    language: tree_sitter::Language,
}

impl GoParser {
    pub fn new() -> Result<Self> {
        let language: tree_sitter::Language = tree_sitter_go::LANGUAGE.into();
        Ok(Self { language })
    }
}

impl Default for GoParser {
    fn default() -> Self {
        Self::new().expect("Failed to initialize GoParser")
    }
}

impl LanguageParser for GoParser {
    fn parse(&self, path: &Path, content: &str) -> Result<FileTags> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&self.language)
            .map_err(|e| RepomapError::Parser(e.to_string()))?;

        let tree = parser
            .parse(content, None)
            .ok_or_else(|| RepomapError::Parser("Failed to parse Go source code".to_string()))?;

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

fn extract_signature(node: &tree_sitter::Node, content: &str) -> String {
    let text = &content[node.byte_range()];
    let cut = if let Some(idx) = text.find('{') {
        &text[..idx]
    } else {
        text
    };
    let trimmed = cut.trim();
    if trimmed.contains('\n') {
        trimmed.lines().next().unwrap_or("").trim().to_string()
    } else {
        trimmed.to_string()
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
    match node.kind() {
        "function_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name")
                && let Ok(name_str) = name_node.utf8_text(content.as_bytes())
                && !name_str.is_empty()
            {
                def_name_ids.insert(name_node.id());
                def_names.insert(name_str.to_string());
                let signature = extract_signature(&node, content);
                let line = node.start_position().row + 1;
                definitions.push(Tag {
                    name: name_str.to_string(),
                    kind: SymbolKind::Function,
                    line,
                    signature,
                });
            }
        }
        "method_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name")
                && let Ok(name_str) = name_node.utf8_text(content.as_bytes())
                && !name_str.is_empty()
            {
                def_name_ids.insert(name_node.id());
                def_names.insert(name_str.to_string());
                let signature = extract_signature(&node, content);
                let line = node.start_position().row + 1;
                definitions.push(Tag {
                    name: name_str.to_string(),
                    kind: SymbolKind::Method,
                    line,
                    signature,
                });
            }
        }
        "type_spec" => {
            if let Some(name_node) = node.child_by_field_name("name")
                && let Ok(name_str) = name_node.utf8_text(content.as_bytes())
                && !name_str.is_empty()
            {
                def_name_ids.insert(name_node.id());
                def_names.insert(name_str.to_string());
                let symbol_kind = if let Some(type_node) = node.child_by_field_name("type") {
                    match type_node.kind() {
                        "struct_type" => SymbolKind::Struct,
                        "interface_type" => SymbolKind::Interface,
                        _ => SymbolKind::TypeAlias,
                    }
                } else {
                    SymbolKind::TypeAlias
                };
                let signature = extract_signature(&node, content);
                let line = node.start_position().row + 1;
                definitions.push(Tag {
                    name: name_str.to_string(),
                    kind: symbol_kind,
                    line,
                    signature,
                });
            }
        }
        _ => {}
    }

    if (node.kind() == "identifier"
        || node.kind() == "type_identifier"
        || node.kind() == "field_identifier"
        || node.kind() == "package_identifier")
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
    fn test_parse_go_definitions() {
        let parser = GoParser::new().unwrap();
        let code = r#"
package main

type UserID int

type Reader interface {
	Read(p []byte) (n int, err error)
}

type Person struct {
	Name string
	Age  int
}

func NewPerson(name string, age int) *Person {
	return &Person{Name: name, Age: age}
}

func (p *Person) Greet(msg string) string {
	return p.Name + ": " + msg
}
"#;
        let tags = parser.parse(Path::new("main.go"), code).unwrap();
        assert_eq!(tags.path, Path::new("main.go"));

        let type_alias = tags
            .definitions
            .iter()
            .find(|t| t.name == "UserID")
            .unwrap();
        assert_eq!(type_alias.kind, SymbolKind::TypeAlias);
        assert_eq!(type_alias.line, 4);
        assert_eq!(type_alias.signature, "UserID int");

        let interface_tag = tags
            .definitions
            .iter()
            .find(|t| t.name == "Reader")
            .unwrap();
        assert_eq!(interface_tag.kind, SymbolKind::Interface);
        assert_eq!(interface_tag.line, 6);
        assert_eq!(interface_tag.signature, "Reader interface");

        let struct_tag = tags
            .definitions
            .iter()
            .find(|t| t.name == "Person")
            .unwrap();
        assert_eq!(struct_tag.kind, SymbolKind::Struct);
        assert_eq!(struct_tag.line, 10);
        assert_eq!(struct_tag.signature, "Person struct");

        let func_tag = tags
            .definitions
            .iter()
            .find(|t| t.name == "NewPerson")
            .unwrap();
        assert_eq!(func_tag.kind, SymbolKind::Function);
        assert_eq!(func_tag.line, 15);
        assert_eq!(
            func_tag.signature,
            "func NewPerson(name string, age int) *Person"
        );

        let method_tag = tags.definitions.iter().find(|t| t.name == "Greet").unwrap();
        assert_eq!(method_tag.kind, SymbolKind::Method);
        assert_eq!(method_tag.line, 19);
        assert_eq!(
            method_tag.signature,
            "func (p *Person) Greet(msg string) string"
        );
    }

    #[test]
    fn test_parse_go_references() {
        let parser = GoParser::new().unwrap();
        let code = r#"
package service

import (
	"fmt"
	"net/http"
)

func HandleRequest(w http.ResponseWriter, r *http.Request) {
	fmt.Println("Handling request")
}
"#;
        let tags = parser.parse(Path::new("service.go"), code).unwrap();
        assert_eq!(tags.definitions.len(), 1);
        assert_eq!(tags.definitions[0].name, "HandleRequest");

        assert!(!tags.references.contains("HandleRequest"));
        assert!(tags.references.contains("http"));
        assert!(tags.references.contains("ResponseWriter"));
        assert!(tags.references.contains("Request"));
        assert!(tags.references.contains("fmt"));
        assert!(tags.references.contains("Println"));
    }

    #[test]
    fn test_parse_go_malformed() {
        let parser = GoParser::new().unwrap();
        let code = r#"
package broken

type Broken struct {
	x int
	syntax error !!!
}

func ValidFunc(x int) int {
	return x + 1
}
"#;
        let tags = parser.parse(Path::new("broken.go"), code).unwrap();
        let valid = tags.definitions.iter().find(|t| t.name == "ValidFunc");
        assert!(valid.is_some());
        assert_eq!(valid.unwrap().kind, SymbolKind::Function);
    }
}
