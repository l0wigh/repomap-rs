use std::collections::HashSet;
use std::path::Path;

use super::LanguageParser;
use crate::{FileTags, RepomapError, Result, SymbolKind, Tag};

pub struct RustParser {
    language: tree_sitter::Language,
}

impl RustParser {
    pub fn new() -> Result<Self> {
        let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        Ok(Self { language })
    }
}

impl Default for RustParser {
    fn default() -> Self {
        Self::new().expect("Failed to initialize RustParser")
    }
}

impl LanguageParser for RustParser {
    fn parse(&self, path: &Path, content: &str) -> Result<FileTags> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&self.language)
            .map_err(|e| RepomapError::Parser(e.to_string()))?;

        let tree = parser
            .parse(content, None)
            .ok_or_else(|| RepomapError::Parser("Failed to parse Rust source code".to_string()))?;

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
            "impl_item" | "trait_item" => return true,
            "function_item" => return false,
            _ => curr = parent.parent(),
        }
    }
    false
}

fn extract_signature(node: &tree_sitter::Node, content: &str) -> String {
    let text = &content[node.byte_range()];
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

fn traverse(
    node: tree_sitter::Node,
    content: &str,
    definitions: &mut Vec<Tag>,
    references: &mut HashSet<String>,
    def_name_ids: &mut HashSet<usize>,
    def_names: &mut HashSet<String>,
) {
    let kind = match node.kind() {
        "function_item" | "function_signature_item" => {
            if is_method(&node) {
                Some(SymbolKind::Method)
            } else {
                Some(SymbolKind::Function)
            }
        }
        "struct_item" => Some(SymbolKind::Struct),
        "enum_item" => Some(SymbolKind::Enum),
        "trait_item" => Some(SymbolKind::Trait),
        "type_item" | "associated_type" => Some(SymbolKind::TypeAlias),
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
    fn test_parse_rust_definitions() {
        let parser = RustParser::new().unwrap();
        let code = r#"
pub struct User {
    id: u64,
    name: String,
}

pub enum Status {
    Active,
    Inactive,
}

pub trait Greeter {
    fn greet(&self) -> String;
}

impl Greeter for User {
    fn greet(&self) -> String {
        format!("Hello, {}", self.name)
    }
}

pub type UserId = u64;

pub fn create_user(id: UserId, name: String) -> User {
    User { id, name }
}
"#;
        let tags = parser.parse(Path::new("src/user.rs"), code).unwrap();
        assert_eq!(tags.path, Path::new("src/user.rs"));

        // User struct
        let user_struct = tags
            .definitions
            .iter()
            .find(|t| t.name == "User")
            .expect("User definition missing");
        assert_eq!(user_struct.kind, SymbolKind::Struct);
        assert_eq!(user_struct.line, 2);
        assert_eq!(user_struct.signature, "pub struct User");

        // Status enum
        let status_enum = tags
            .definitions
            .iter()
            .find(|t| t.name == "Status")
            .expect("Status definition missing");
        assert_eq!(status_enum.kind, SymbolKind::Enum);
        assert_eq!(status_enum.line, 7);
        assert_eq!(status_enum.signature, "pub enum Status");

        // Greeter trait
        let greeter_trait = tags
            .definitions
            .iter()
            .find(|t| t.name == "Greeter")
            .expect("Greeter definition missing");
        assert_eq!(greeter_trait.kind, SymbolKind::Trait);
        assert_eq!(greeter_trait.line, 12);
        assert_eq!(greeter_trait.signature, "pub trait Greeter");

        // Greeter method in trait
        let trait_method = tags
            .definitions
            .iter()
            .find(|t| t.name == "greet" && t.line == 13)
            .expect("Trait method greet missing");
        assert_eq!(trait_method.kind, SymbolKind::Method);
        assert_eq!(trait_method.signature, "fn greet(&self) -> String");

        // Greeter method in impl
        let impl_method = tags
            .definitions
            .iter()
            .find(|t| t.name == "greet" && t.line == 17)
            .expect("Impl method greet missing");
        assert_eq!(impl_method.kind, SymbolKind::Method);
        assert_eq!(impl_method.signature, "fn greet(&self) -> String");

        // UserId type alias
        let user_id_alias = tags
            .definitions
            .iter()
            .find(|t| t.name == "UserId")
            .expect("UserId definition missing");
        assert_eq!(user_id_alias.kind, SymbolKind::TypeAlias);
        assert_eq!(user_id_alias.line, 22);
        assert_eq!(user_id_alias.signature, "pub type UserId = u64");

        // create_user function
        let create_user_fn = tags
            .definitions
            .iter()
            .find(|t| t.name == "create_user")
            .expect("create_user definition missing");
        assert_eq!(create_user_fn.kind, SymbolKind::Function);
        assert_eq!(create_user_fn.line, 24);
        assert_eq!(
            create_user_fn.signature,
            "pub fn create_user(id: UserId, name: String) -> User"
        );
    }

    #[test]
    fn test_parse_rust_references() {
        let parser = RustParser::new().unwrap();
        let code = r#"
use std::collections::HashMap;
use external_crate::ExternalService;

pub fn handle_request(req: Request) -> Result<Response, MyError> {
    let service = ExternalService::new();
    let mut map: HashMap<String, Value> = HashMap::new();
    service.process(req)
}
"#;
        let tags = parser.parse(Path::new("src/handler.rs"), code).unwrap();

        // Definition should be handle_request
        assert_eq!(tags.definitions.len(), 1);
        assert_eq!(tags.definitions[0].name, "handle_request");

        // References must not contain the definition name
        assert!(!tags.references.contains("handle_request"));

        // References should contain referenced types and functions
        assert!(tags.references.contains("ExternalService"));
        assert!(tags.references.contains("HashMap"));
        assert!(tags.references.contains("Request"));
        assert!(tags.references.contains("Response"));
        assert!(tags.references.contains("MyError"));
        assert!(tags.references.contains("process"));
    }

    #[test]
    fn test_parse_rust_malformed_syntax() {
        let parser = RustParser::new().unwrap();
        let code = r#"
pub struct Broken {
    pub x: u32,
    syntax error !!! {{{{
}

pub fn valid_function(a: i32) -> i32 {
    a + 1
}

pub enum IncompleteEnum {
"#;
        let tags = parser.parse(Path::new("src/broken.rs"), code).unwrap();

        // Should not crash and should extract the valid_function definition
        let valid_fn = tags.definitions.iter().find(|t| t.name == "valid_function");
        assert!(valid_fn.is_some());
        assert_eq!(valid_fn.unwrap().kind, SymbolKind::Function);
    }
}
