use std::collections::HashSet;
use std::path::Path;

use super::LanguageParser;
use crate::{FileTags, RepomapError, Result, SymbolKind, Tag};

pub struct PhpParser {
    language: tree_sitter::Language,
}

impl PhpParser {
    pub fn new() -> Result<Self> {
        let language: tree_sitter::Language = tree_sitter_php::LANGUAGE_PHP.into();
        Ok(Self { language })
    }
}

impl Default for PhpParser {
    fn default() -> Self {
        Self::new().expect("Failed to initialize PhpParser")
    }
}

impl LanguageParser for PhpParser {
    fn parse(&self, path: &Path, content: &str) -> Result<FileTags> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&self.language)
            .map_err(|e| RepomapError::Parser(e.to_string()))?;

        let tree = parser
            .parse(content, None)
            .ok_or_else(|| RepomapError::Parser("Failed to parse PHP source code".to_string()))?;

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
    let cut = if let Some(idx) = text.find(['{', ';']) {
        &text[..idx]
    } else {
        text
    };
    let trimmed = cut.trim();
    if trimmed.contains('\n') {
        for line in trimmed.lines() {
            let l = line.trim();
            if !l.is_empty()
                && !l.starts_with('#')
                && !l.starts_with("//")
                && !l.starts_with("/*")
                && !l.starts_with('*')
            {
                return l.to_string();
            }
        }
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
        "class_declaration" => Some(SymbolKind::Class),
        "interface_declaration" => Some(SymbolKind::Interface),
        "trait_declaration" => Some(SymbolKind::Trait),
        "enum_declaration" => Some(SymbolKind::Enum),
        "function_definition" => Some(SymbolKind::Function),
        "method_declaration" => Some(SymbolKind::Method),
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
        let line = name_node.start_position().row + 1;
        definitions.push(Tag {
            name: name_str.to_string(),
            kind: symbol_kind,
            line,
            signature,
        });
    }

    if (node.kind() == "name" || node.kind() == "variable_name")
        && !def_name_ids.contains(&node.id())
        && let Ok(ident_text) = node.utf8_text(content.as_bytes())
        && !ident_text.is_empty()
    {
        let clean = ident_text.trim_start_matches('$');
        if !clean.is_empty() {
            references.insert(clean.to_string());
        }
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
    fn test_parse_php_definitions() {
        let parser = PhpParser::new().unwrap();
        let code = r#"<?php
namespace App\Services;

interface UserService {
    public function findById(int $id): ?User;
}

trait LoggerTrait {
    public function log(string $msg): void {}
}

enum Status: string {
    case Pending = 'pending';
    case Active = 'active';
}

class UserServiceImpl implements UserService {
    use LoggerTrait;

    public function findById(int $id): ?User {
        $this->log("finding");
        return null;
    }
}

function globalHelper(): void {}
"#;
        let tags = parser.parse(Path::new("src/Service.php"), code).unwrap();
        assert_eq!(tags.path, Path::new("src/Service.php"));

        let iface = tags
            .definitions
            .iter()
            .find(|t| t.name == "UserService")
            .expect("UserService missing");
        assert_eq!(iface.kind, SymbolKind::Interface);
        assert_eq!(iface.line, 4);
        assert_eq!(iface.signature, "interface UserService");

        let iface_method = tags
            .definitions
            .iter()
            .find(|t| t.name == "findById" && t.line == 5)
            .expect("findById interface method missing");
        assert_eq!(iface_method.kind, SymbolKind::Method);
        assert_eq!(iface_method.line, 5);
        assert_eq!(
            iface_method.signature,
            "public function findById(int $id): ?User"
        );

        let trait_def = tags
            .definitions
            .iter()
            .find(|t| t.name == "LoggerTrait")
            .expect("LoggerTrait missing");
        assert_eq!(trait_def.kind, SymbolKind::Trait);
        assert_eq!(trait_def.line, 8);
        assert_eq!(trait_def.signature, "trait LoggerTrait");

        let trait_method = tags
            .definitions
            .iter()
            .find(|t| t.name == "log")
            .expect("log method missing");
        assert_eq!(trait_method.kind, SymbolKind::Method);
        assert_eq!(trait_method.line, 9);
        assert_eq!(
            trait_method.signature,
            "public function log(string $msg): void"
        );

        let enum_def = tags
            .definitions
            .iter()
            .find(|t| t.name == "Status")
            .expect("Status enum missing");
        assert_eq!(enum_def.kind, SymbolKind::Enum);
        assert_eq!(enum_def.line, 12);
        assert_eq!(enum_def.signature, "enum Status: string");

        let class_def = tags
            .definitions
            .iter()
            .find(|t| t.name == "UserServiceImpl")
            .expect("UserServiceImpl class missing");
        assert_eq!(class_def.kind, SymbolKind::Class);
        assert_eq!(class_def.line, 17);
        assert_eq!(
            class_def.signature,
            "class UserServiceImpl implements UserService"
        );

        let class_method = tags
            .definitions
            .iter()
            .find(|t| t.name == "findById" && t.line == 20)
            .expect("findById class method missing");
        assert_eq!(class_method.kind, SymbolKind::Method);
        assert_eq!(class_method.line, 20);
        assert_eq!(
            class_method.signature,
            "public function findById(int $id): ?User"
        );

        let func_def = tags
            .definitions
            .iter()
            .find(|t| t.name == "globalHelper")
            .expect("globalHelper function missing");
        assert_eq!(func_def.kind, SymbolKind::Function);
        assert_eq!(func_def.line, 26);
        assert_eq!(func_def.signature, "function globalHelper(): void");
    }

    #[test]
    fn test_parse_php_references() {
        let parser = PhpParser::new().unwrap();
        let code = r#"<?php
use App\Models\User;
use App\Database\Connection;

function processUser(User $u, Connection $conn) {
    $conn->query("SELECT 1");
    Logger::info("processed", $u->id);
}
"#;
        let tags = parser.parse(Path::new("src/process.php"), code).unwrap();

        assert_eq!(tags.definitions.len(), 1);
        assert_eq!(tags.definitions[0].name, "processUser");

        assert!(!tags.references.contains("processUser"));
        assert!(tags.references.contains("User"));
        assert!(tags.references.contains("Connection"));
        assert!(tags.references.contains("Logger"));
        assert!(tags.references.contains("info"));
        assert!(tags.references.contains("query"));
    }

    #[test]
    fn test_parse_php_malformed() {
        let parser = PhpParser::new().unwrap();
        let code = r#"<?php
class IncompleteClass {
    public function { invalid syntax
}

function validPhpFunction(int $x): int {
    return $x * 2;
}
"#;
        let tags = parser.parse(Path::new("src/malformed.php"), code).unwrap();
        let valid = tags
            .definitions
            .iter()
            .find(|t| t.name == "validPhpFunction");
        assert!(valid.is_some());
        assert_eq!(valid.unwrap().kind, SymbolKind::Function);
    }
}
