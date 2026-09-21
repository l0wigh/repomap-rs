use std::collections::HashSet;
use std::path::Path;

use super::LanguageParser;
use crate::{FileTags, RepomapError, Result, SymbolKind, Tag};

pub struct CParser {
    language: tree_sitter::Language,
}

impl CParser {
    pub fn new() -> Result<Self> {
        let language: tree_sitter::Language = tree_sitter_c::LANGUAGE.into();
        Ok(Self { language })
    }
}

impl Default for CParser {
    fn default() -> Self {
        Self::new().expect("Failed to initialize CParser")
    }
}

impl LanguageParser for CParser {
    fn parse(&self, path: &Path, content: &str) -> Result<FileTags> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&self.language)
            .map_err(|e| RepomapError::Parser(e.to_string()))?;

        let tree = parser
            .parse(content, None)
            .ok_or_else(|| RepomapError::Parser("Failed to parse C source code".to_string()))?;

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
    let cut = match node.kind() {
        "function_definition" => {
            if let Some(idx) = text.find('{') {
                &text[..idx]
            } else {
                text
            }
        }
        "struct_specifier" | "union_specifier" | "enum_specifier" => {
            if let Some(idx) = text.find(['{', ';']) {
                &text[..idx]
            } else {
                text
            }
        }
        "type_definition" => {
            if let Some(idx) = text.find(';') {
                &text[..idx]
            } else {
                text
            }
        }
        _ => {
            if let Some(idx) = text.find(['{', ';']) {
                &text[..idx]
            } else {
                text
            }
        }
    };
    let trimmed = cut.trim();
    if trimmed.contains('\n') {
        trimmed.lines().next().unwrap_or("").trim().to_string()
    } else {
        trimmed.to_string()
    }
}

fn find_declarator_name_node<'a>(node: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    match node.kind() {
        "identifier" | "type_identifier" | "primitive_type" | "field_identifier" => Some(node),
        _ => {
            if let Some(declarator) = node.child_by_field_name("declarator") {
                find_declarator_name_node(declarator)
            } else {
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        let k = child.kind();
                        if k == "identifier"
                            || k == "type_identifier"
                            || k == "primitive_type"
                            || k == "field_identifier"
                        {
                            return Some(child);
                        }
                        if let Some(found) = find_declarator_name_node(child) {
                            return Some(found);
                        }
                    }
                }
                None
            }
        }
    }
}

fn is_definition_specifier(node: &tree_sitter::Node) -> bool {
    if node.child_by_field_name("body").is_some() {
        return true;
    }
    if let Some(parent) = node.parent()
        && parent.kind() == "declaration"
        && parent.child_by_field_name("declarator").is_none()
    {
        return true;
    }
    false
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
        "function_definition" => {
            if let Some(declarator) = node.child_by_field_name("declarator")
                && let Some(name_node) = find_declarator_name_node(declarator)
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
        "struct_specifier" | "union_specifier" => {
            if let Some(name_node) = node.child_by_field_name("name")
                && is_definition_specifier(&node)
                && let Ok(name_str) = name_node.utf8_text(content.as_bytes())
                && !name_str.is_empty()
            {
                def_name_ids.insert(name_node.id());
                def_names.insert(name_str.to_string());
                let signature = extract_signature(&node, content);
                let line = node.start_position().row + 1;
                definitions.push(Tag {
                    name: name_str.to_string(),
                    kind: SymbolKind::Struct,
                    line,
                    signature,
                });
            }
        }
        "enum_specifier" => {
            if let Some(name_node) = node.child_by_field_name("name")
                && is_definition_specifier(&node)
                && let Ok(name_str) = name_node.utf8_text(content.as_bytes())
                && !name_str.is_empty()
            {
                def_name_ids.insert(name_node.id());
                def_names.insert(name_str.to_string());
                let signature = extract_signature(&node, content);
                let line = node.start_position().row + 1;
                definitions.push(Tag {
                    name: name_str.to_string(),
                    kind: SymbolKind::Enum,
                    line,
                    signature,
                });
            }
        }
        "type_definition" => {
            let mut declarators = Vec::new();
            for i in 0..node.child_count() {
                if node.field_name_for_child(i) == Some("declarator")
                    && let Some(child) = node.child(i)
                {
                    declarators.push(child);
                }
            }
            if declarators.is_empty()
                && let Some(d) = node.child_by_field_name("declarator")
            {
                declarators.push(d);
            }

            for decl in declarators {
                if let Some(name_node) = find_declarator_name_node(decl)
                    && let Ok(name_str) = name_node.utf8_text(content.as_bytes())
                    && !name_str.is_empty()
                {
                    def_name_ids.insert(name_node.id());
                    def_names.insert(name_str.to_string());
                    let signature = extract_signature(&node, content);
                    let line = node.start_position().row + 1;
                    definitions.push(Tag {
                        name: name_str.to_string(),
                        kind: SymbolKind::TypeAlias,
                        line,
                        signature,
                    });
                }
            }
        }
        _ => {}
    }

    if (node.kind() == "identifier"
        || node.kind() == "type_identifier"
        || node.kind() == "field_identifier")
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
    fn test_parse_c_functions() {
        let parser = CParser::new().unwrap();
        let code = r#"
int add(int a, int b) {
    return a + b;
}

char *get_name(User *u) {
    return u->name;
}

void process_data(int *buffer,
                  int size)
{
    // implementation
}
"#;
        let tags = parser.parse(Path::new("src/math.c"), code).unwrap();
        assert_eq!(tags.path, Path::new("src/math.c"));

        let add_fn = tags
            .definitions
            .iter()
            .find(|t| t.name == "add")
            .expect("add definition missing");
        assert_eq!(add_fn.kind, SymbolKind::Function);
        assert_eq!(add_fn.line, 2);
        assert_eq!(add_fn.signature, "int add(int a, int b)");

        let get_name_fn = tags
            .definitions
            .iter()
            .find(|t| t.name == "get_name")
            .expect("get_name definition missing");
        assert_eq!(get_name_fn.kind, SymbolKind::Function);
        assert_eq!(get_name_fn.line, 6);
        assert_eq!(get_name_fn.signature, "char *get_name(User *u)");

        let process_fn = tags
            .definitions
            .iter()
            .find(|t| t.name == "process_data")
            .expect("process_data definition missing");
        assert_eq!(process_fn.kind, SymbolKind::Function);
        assert_eq!(process_fn.line, 10);
        assert_eq!(process_fn.signature, "void process_data(int *buffer,");
    }

    #[test]
    fn test_parse_c_structs_unions_enums() {
        let parser = CParser::new().unwrap();
        let code = r#"
struct Point {
    int x;
    int y;
};

union Data {
    int i;
    float f;
};

enum Color {
    RED,
    GREEN,
    BLUE
};
"#;
        let tags = parser.parse(Path::new("src/types.h"), code).unwrap();

        let pt = tags
            .definitions
            .iter()
            .find(|t| t.name == "Point")
            .expect("Point struct missing");
        assert_eq!(pt.kind, SymbolKind::Struct);
        assert_eq!(pt.line, 2);
        assert_eq!(pt.signature, "struct Point");

        let data = tags
            .definitions
            .iter()
            .find(|t| t.name == "Data")
            .expect("Data union missing");
        assert_eq!(data.kind, SymbolKind::Struct);
        assert_eq!(data.line, 7);
        assert_eq!(data.signature, "union Data");

        let color = tags
            .definitions
            .iter()
            .find(|t| t.name == "Color")
            .expect("Color enum missing");
        assert_eq!(color.kind, SymbolKind::Enum);
        assert_eq!(color.line, 12);
        assert_eq!(color.signature, "enum Color");
    }

    #[test]
    fn test_parse_c_typedefs() {
        let parser = CParser::new().unwrap();
        let code = r#"
typedef unsigned long ulong;
typedef struct Point Point_t;
typedef struct {
    int x;
    int y;
} Vector2D;
typedef void (*Callback)(int status);
"#;
        let tags = parser.parse(Path::new("src/aliases.h"), code).unwrap();

        let ulong_def = tags
            .definitions
            .iter()
            .find(|t| t.name == "ulong")
            .expect("ulong missing");
        assert_eq!(ulong_def.kind, SymbolKind::TypeAlias);
        assert_eq!(ulong_def.line, 2);

        let point_t = tags
            .definitions
            .iter()
            .find(|t| t.name == "Point_t")
            .expect("Point_t missing");
        assert_eq!(point_t.kind, SymbolKind::TypeAlias);
        assert_eq!(point_t.line, 3);

        let vec2d = tags
            .definitions
            .iter()
            .find(|t| t.name == "Vector2D")
            .expect("Vector2D missing");
        assert_eq!(vec2d.kind, SymbolKind::TypeAlias);
        assert_eq!(vec2d.line, 4);

        let cb = tags
            .definitions
            .iter()
            .find(|t| t.name == "Callback")
            .expect("Callback missing");
        assert_eq!(cb.kind, SymbolKind::TypeAlias);
        assert_eq!(cb.line, 8);
    }

    #[test]
    fn test_parse_c_references() {
        let parser = CParser::new().unwrap();
        let code = r#"
#include "service.h"

int handle_request(Request *req) {
    Response res;
    res.status = OK;
    send_response(&res);
    return 0;
}
"#;
        let tags = parser.parse(Path::new("src/handler.c"), code).unwrap();

        // handle_request is defined in this file
        assert!(tags.definitions.iter().any(|d| d.name == "handle_request"));
        // Definitions must not be in references
        assert!(!tags.references.contains("handle_request"));

        // References should include external types, functions, and fields
        assert!(tags.references.contains("Request"));
        assert!(tags.references.contains("Response"));
        assert!(tags.references.contains("req"));
        assert!(tags.references.contains("res"));
        assert!(tags.references.contains("status"));
        assert!(tags.references.contains("OK"));
        assert!(tags.references.contains("send_response"));
    }

    #[test]
    fn test_parse_c_malformed() {
        let parser = CParser::new().unwrap();
        let code = r#"
struct Incomplete {
    int valid_field;
    syntax error here ;;;; {{{{
}

int working_func(int x) {
    return x * 2;
}

invalid c syntax &&&&&
"#;
        let tags = parser.parse(Path::new("src/malformed.c"), code).unwrap();

        let func = tags
            .definitions
            .iter()
            .find(|t| t.name == "working_func")
            .expect("working_func should still be parsed despite syntax errors");
        assert_eq!(func.kind, SymbolKind::Function);
    }
}
