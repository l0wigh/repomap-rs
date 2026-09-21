use std::collections::HashSet;
use std::path::Path;

use super::LanguageParser;
use crate::{FileTags, RepomapError, Result, SymbolKind, Tag};

pub struct CppParser {
    language: tree_sitter::Language,
}

impl CppParser {
    pub fn new() -> Result<Self> {
        let language: tree_sitter::Language = tree_sitter_cpp::LANGUAGE.into();
        Ok(Self { language })
    }
}

impl Default for CppParser {
    fn default() -> Self {
        Self::new().expect("Failed to initialize CppParser")
    }
}

impl LanguageParser for CppParser {
    fn parse(&self, path: &Path, content: &str) -> Result<FileTags> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&self.language)
            .map_err(|e| RepomapError::Parser(e.to_string()))?;

        let tree = parser
            .parse(content, None)
            .ok_or_else(|| RepomapError::Parser("Failed to parse C++ source code".to_string()))?;

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
    match node.kind() {
        "alias_declaration" | "type_definition" => {
            let cut = if let Some(idx) = text.find(';') {
                &text[..idx]
            } else {
                text
            };
            let trimmed = cut.trim();
            let collapsed: String = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
            if collapsed.len() > 120 {
                format!("{}...", &collapsed[..117])
            } else {
                collapsed
            }
        }
        "function_definition" => {
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
        "class_specifier" | "struct_specifier" | "enum_specifier" | "namespace_definition" => {
            let cut = if let Some(idx) = text.find(['{', ';']) {
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
        _ => {
            let cut = if let Some(idx) = text.find(['{', ';']) {
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
    }
}

fn find_declarator_name_node<'a>(node: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    match node.kind() {
        "identifier"
        | "field_identifier"
        | "type_identifier"
        | "qualified_identifier"
        | "destructor_name"
        | "operator_name" => Some(node),
        _ => {
            if let Some(declarator) = node.child_by_field_name("declarator") {
                find_declarator_name_node(declarator)
            } else {
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        let k = child.kind();
                        if k == "identifier"
                            || k == "field_identifier"
                            || k == "type_identifier"
                            || k == "qualified_identifier"
                            || k == "destructor_name"
                            || k == "operator_name"
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

fn is_method(node: &tree_sitter::Node, name_node: &tree_sitter::Node) -> bool {
    if name_node.kind() == "qualified_identifier" {
        return true;
    }
    let mut curr = node.parent();
    while let Some(parent) = curr {
        match parent.kind() {
            "class_specifier" | "struct_specifier" => return true,
            "function_definition" => return false,
            _ => curr = parent.parent(),
        }
    }
    false
}

fn is_definition_specifier(node: &tree_sitter::Node) -> bool {
    if node.child_by_field_name("body").is_some() {
        return true;
    }
    if let Some(parent) = node.parent() {
        if parent.kind() == "declaration" && parent.child_by_field_name("declarator").is_none() {
            return true;
        }
        if parent.kind() == "template_declaration" {
            return true;
        }
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
        "class_specifier" => {
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
                    kind: SymbolKind::Class,
                    line,
                    signature,
                });
            }
        }
        "struct_specifier" => {
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
        "namespace_definition" => {
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
                    kind: SymbolKind::Class,
                    line,
                    signature,
                });
            }
        }
        "function_definition" => {
            if let Some(declarator) = node.child_by_field_name("declarator")
                && let Some(name_node) = find_declarator_name_node(declarator)
                && let Ok(name_str) = name_node.utf8_text(content.as_bytes())
                && !name_str.is_empty()
            {
                def_name_ids.insert(name_node.id());
                if name_node.kind() == "qualified_identifier"
                    && let Some(child_name) = name_node.child_by_field_name("name")
                {
                    def_name_ids.insert(child_name.id());
                    if let Ok(base_name) = child_name.utf8_text(content.as_bytes()) {
                        def_names.insert(base_name.to_string());
                    }
                }
                def_names.insert(name_str.to_string());
                let kind = if is_method(&node, &name_node) {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                };
                let signature = extract_signature(&node, content);
                let line = node.start_position().row + 1;
                definitions.push(Tag {
                    name: name_str.to_string(),
                    kind,
                    line,
                    signature,
                });
            }
        }
        "alias_declaration" => {
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
                    kind: SymbolKind::TypeAlias,
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
        _ => {}
    }

    if (node.kind() == "identifier"
        || node.kind() == "type_identifier"
        || node.kind() == "field_identifier"
        || node.kind() == "namespace_identifier")
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
    fn test_parse_cpp_classes() {
        let parser = CppParser::new().unwrap();
        let code = r#"
class User {
private:
    int id;
};

struct Config {
    bool debug;
};

enum class Status {
    Active,
    Inactive
};

using UserId = int;
typedef double Price;
"#;
        let tags = parser.parse(Path::new("src/user.hpp"), code).unwrap();

        let user = tags
            .definitions
            .iter()
            .find(|t| t.name == "User")
            .expect("User class missing");
        assert_eq!(user.kind, SymbolKind::Class);
        assert_eq!(user.line, 2);
        assert_eq!(user.signature, "class User");

        let config = tags
            .definitions
            .iter()
            .find(|t| t.name == "Config")
            .expect("Config struct missing");
        assert_eq!(config.kind, SymbolKind::Struct);
        assert_eq!(config.line, 7);
        assert_eq!(config.signature, "struct Config");

        let status = tags
            .definitions
            .iter()
            .find(|t| t.name == "Status")
            .expect("Status enum missing");
        assert_eq!(status.kind, SymbolKind::Enum);
        assert_eq!(status.line, 11);
        assert_eq!(status.signature, "enum class Status");

        let user_id = tags
            .definitions
            .iter()
            .find(|t| t.name == "UserId")
            .expect("UserId alias missing");
        assert_eq!(user_id.kind, SymbolKind::TypeAlias);
        assert_eq!(user_id.line, 16);

        let price = tags
            .definitions
            .iter()
            .find(|t| t.name == "Price")
            .expect("Price typedef missing");
        assert_eq!(price.kind, SymbolKind::TypeAlias);
        assert_eq!(price.line, 17);
    }

    #[test]
    fn test_parse_cpp_methods() {
        let parser = CppParser::new().unwrap();
        let code = r#"
class Controller {
public:
    void handleRequest() {
        // in-class method
    }
};

void Controller::shutdown() {
    // qualified method
}

int calculateTotal(int a, int b) {
    return a + b;
}
"#;
        let tags = parser.parse(Path::new("src/controller.cpp"), code).unwrap();

        let in_class = tags
            .definitions
            .iter()
            .find(|t| t.name == "handleRequest")
            .expect("handleRequest missing");
        assert_eq!(in_class.kind, SymbolKind::Method);
        assert_eq!(in_class.line, 4);
        assert_eq!(in_class.signature, "void handleRequest()");

        let qualified = tags
            .definitions
            .iter()
            .find(|t| t.name == "Controller::shutdown")
            .expect("Controller::shutdown missing");
        assert_eq!(qualified.kind, SymbolKind::Method);
        assert_eq!(qualified.line, 9);
        assert_eq!(qualified.signature, "void Controller::shutdown()");

        let free_fn = tags
            .definitions
            .iter()
            .find(|t| t.name == "calculateTotal")
            .expect("calculateTotal missing");
        assert_eq!(free_fn.kind, SymbolKind::Function);
        assert_eq!(free_fn.line, 13);
        assert_eq!(free_fn.signature, "int calculateTotal(int a, int b)");
    }

    #[test]
    fn test_parse_cpp_namespaces() {
        let parser = CppParser::new().unwrap();
        let code = r#"
namespace Networking {
    void connect() {
        // connection logic
    }
}
"#;
        let tags = parser.parse(Path::new("src/net.cpp"), code).unwrap();

        let ns = tags
            .definitions
            .iter()
            .find(|t| t.name == "Networking")
            .expect("Networking namespace missing");
        assert_eq!(ns.kind, SymbolKind::Class);
        assert_eq!(ns.line, 2);
        assert_eq!(ns.signature, "namespace Networking");

        let conn = tags
            .definitions
            .iter()
            .find(|t| t.name == "connect")
            .expect("connect function missing");
        assert_eq!(conn.kind, SymbolKind::Function);
        assert_eq!(conn.line, 3);
    }

    #[test]
    fn test_parse_cpp_templates() {
        let parser = CppParser::new().unwrap();
        let code = r#"
template <typename T>
class Buffer {
    T* data;
};

template <typename T>
T clamp(T val, T min, T max) {
    if (val < min) return min;
    if (val > max) return max;
    return val;
}
"#;
        let tags = parser.parse(Path::new("src/buffer.hpp"), code).unwrap();

        let buf = tags
            .definitions
            .iter()
            .find(|t| t.name == "Buffer")
            .expect("Buffer class missing");
        assert_eq!(buf.kind, SymbolKind::Class);
        assert_eq!(buf.line, 3);
        assert_eq!(buf.signature, "class Buffer");

        let clamp_fn = tags
            .definitions
            .iter()
            .find(|t| t.name == "clamp")
            .expect("clamp template function missing");
        assert_eq!(clamp_fn.kind, SymbolKind::Function);
        assert_eq!(clamp_fn.line, 8);
        assert_eq!(clamp_fn.signature, "T clamp(T val, T min, T max)");
    }

    #[test]
    fn test_parse_cpp_references() {
        let parser = CppParser::new().unwrap();
        let code = r#"
#include "service.hpp"

void run_task(ExternalService* svc) {
    Logger::info("starting task");
    svc->execute();
}
"#;
        let tags = parser.parse(Path::new("src/task.cpp"), code).unwrap();

        // run_task is defined in this file
        assert!(tags.definitions.iter().any(|d| d.name == "run_task"));
        assert!(!tags.references.contains("run_task"));

        // References must contain external types, namespaces, and functions
        assert!(tags.references.contains("ExternalService"));
        assert!(tags.references.contains("Logger"));
        assert!(tags.references.contains("info"));
        assert!(tags.references.contains("execute"));
    }

    #[test]
    fn test_parse_cpp_malformed() {
        let parser = CppParser::new().unwrap();
        let code = r#"
class BrokenClass {
    syntax error here ;;;; {{{{
};

void valid_func() {
    int x = 42;
}

invalid cpp code >>>> <<<<
"#;
        let tags = parser.parse(Path::new("src/broken.cpp"), code).unwrap();

        let func = tags
            .definitions
            .iter()
            .find(|t| t.name == "valid_func")
            .expect("valid_func should be recovered");
        assert_eq!(func.kind, SymbolKind::Function);
    }

    #[test]
    fn test_parse_cpp_multiline_type_alias() {
        let parser = CppParser::new().unwrap();
        let code = r#"
template <typename T>
using CallbackHandler =
    std::function<void(const T&, int)>;
"#;
        let tags = parser.parse(Path::new("src/types.hpp"), code).unwrap();
        let alias = tags
            .definitions
            .iter()
            .find(|t| t.name == "CallbackHandler")
            .expect("CallbackHandler alias missing");
        assert_eq!(alias.kind, SymbolKind::TypeAlias);
        assert_eq!(
            alias.signature,
            "using CallbackHandler = std::function<void(const T&, int)>"
        );
        assert!(!alias.signature.ends_with('='));
    }
}
