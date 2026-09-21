use std::collections::HashSet;
use std::path::Path;

use super::LanguageParser;
use crate::{FileTags, RepomapError, Result, SymbolKind, Tag};

pub struct KotlinParser {
    language: tree_sitter::Language,
}

#[link(name = "parser")]
unsafe extern "C" {
    fn tree_sitter_kotlin() -> *const ();
}

impl KotlinParser {
    pub fn new() -> Result<Self> {
        let ptr = unsafe { tree_sitter_kotlin() };
        let language: tree_sitter::Language = unsafe { std::mem::transmute(ptr) };
        Ok(Self { language })
    }
}

impl Default for KotlinParser {
    fn default() -> Self {
        Self::new().expect("Failed to initialize KotlinParser")
    }
}

impl LanguageParser for KotlinParser {
    fn parse(&self, path: &Path, content: &str) -> Result<FileTags> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&self.language)
            .map_err(|e| RepomapError::Parser(e.to_string()))?;

        let tree = parser.parse(content, None).ok_or_else(|| {
            RepomapError::Parser("Failed to parse Kotlin source code".to_string())
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

fn extract_signature(node: &tree_sitter::Node, content: &str) -> String {
    let text = &content[node.byte_range()];
    let cut = if let Some(idx) = text.find(['{', ';', '=']) {
        &text[..idx]
    } else {
        text
    };
    let trimmed = cut.trim();
    if trimmed.contains('\n') {
        for line in trimmed.lines() {
            let l = line.trim();
            if !l.is_empty()
                && !l.starts_with('@')
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

fn is_method(node: &tree_sitter::Node) -> bool {
    let mut curr = node.parent();
    while let Some(parent) = curr {
        match parent.kind() {
            "class_body" | "class_declaration" | "object_declaration" => return true,
            "source_file" => return false,
            _ => curr = parent.parent(),
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
    let kind = match node.kind() {
        "class_declaration" => {
            let mut is_interface = false;
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i)
                    && (child.kind() == "interface"
                        || child.utf8_text(content.as_bytes()).unwrap_or("") == "interface")
                {
                    is_interface = true;
                    break;
                }
            }
            if is_interface {
                Some(SymbolKind::Interface)
            } else {
                Some(SymbolKind::Class)
            }
        }
        "object_declaration" => Some(SymbolKind::Class),
        "function_declaration" => {
            if is_method(&node) {
                Some(SymbolKind::Method)
            } else {
                Some(SymbolKind::Function)
            }
        }
        _ => None,
    };

    let name_node = node.child_by_field_name("name").or_else(|| {
        if node.kind() == "class_declaration"
            || node.kind() == "object_declaration"
            || node.kind() == "function_declaration"
        {
            for i in 0..node.child_count() {
                if let Some(c) = node.child(i)
                    && (c.kind() == "type_identifier" || c.kind() == "simple_identifier")
                {
                    return Some(c);
                }
            }
        }
        None
    });

    if let Some(symbol_kind) = kind
        && let Some(name_n) = name_node
        && let Ok(name_str) = name_n.utf8_text(content.as_bytes())
        && !name_str.is_empty()
    {
        def_name_ids.insert(name_n.id());
        def_names.insert(name_str.to_string());
        let signature = extract_signature(&node, content);
        let line = name_n.start_position().row + 1;
        definitions.push(Tag {
            name: name_str.to_string(),
            kind: symbol_kind,
            line,
            signature,
        });
    }

    if (node.kind() == "simple_identifier" || node.kind() == "type_identifier")
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
    fn test_parse_kotlin_definitions() {
        let parser = KotlinParser::new().unwrap();
        let code = r#"package com.example

interface Service {
    fun execute(): Boolean
}

data class User(val id: Long, val name: String)

object AppConfig {
    val version = "1.0"
    fun getInfo(): String = version
}

class UserServiceImpl : Service {
    override fun execute(): Boolean {
        return true
    }
}

fun topLevelHelper(param: Int): Int {
    return param + 1
}
"#;
        let tags = parser.parse(Path::new("src/Service.kt"), code).unwrap();
        assert_eq!(tags.path, Path::new("src/Service.kt"));

        let iface = tags
            .definitions
            .iter()
            .find(|t| t.name == "Service")
            .expect("Service interface missing");
        assert_eq!(iface.kind, SymbolKind::Interface);
        assert_eq!(iface.line, 3);
        assert_eq!(iface.signature, "interface Service");

        let iface_method = tags
            .definitions
            .iter()
            .find(|t| t.name == "execute" && t.line == 4)
            .expect("execute interface method missing");
        assert_eq!(iface_method.kind, SymbolKind::Method);
        assert_eq!(iface_method.line, 4);
        assert_eq!(iface_method.signature, "fun execute(): Boolean");

        let data_class = tags
            .definitions
            .iter()
            .find(|t| t.name == "User")
            .expect("User data class missing");
        assert_eq!(data_class.kind, SymbolKind::Class);
        assert_eq!(data_class.line, 7);
        assert_eq!(
            data_class.signature,
            "data class User(val id: Long, val name: String)"
        );

        let obj = tags
            .definitions
            .iter()
            .find(|t| t.name == "AppConfig")
            .expect("AppConfig object missing");
        assert_eq!(obj.kind, SymbolKind::Class);
        assert_eq!(obj.line, 9);
        assert_eq!(obj.signature, "object AppConfig");

        let obj_method = tags
            .definitions
            .iter()
            .find(|t| t.name == "getInfo")
            .expect("getInfo method missing");
        assert_eq!(obj_method.kind, SymbolKind::Method);
        assert_eq!(obj_method.line, 11);
        assert_eq!(obj_method.signature, "fun getInfo(): String");

        let class_def = tags
            .definitions
            .iter()
            .find(|t| t.name == "UserServiceImpl")
            .expect("UserServiceImpl class missing");
        assert_eq!(class_def.kind, SymbolKind::Class);
        assert_eq!(class_def.line, 14);
        assert_eq!(class_def.signature, "class UserServiceImpl : Service");

        let class_method = tags
            .definitions
            .iter()
            .find(|t| t.name == "execute" && t.line == 15)
            .expect("execute class method missing");
        assert_eq!(class_method.kind, SymbolKind::Method);
        assert_eq!(class_method.line, 15);
        assert_eq!(class_method.signature, "override fun execute(): Boolean");

        let func_def = tags
            .definitions
            .iter()
            .find(|t| t.name == "topLevelHelper")
            .expect("topLevelHelper function missing");
        assert_eq!(func_def.kind, SymbolKind::Function);
        assert_eq!(func_def.line, 20);
        assert_eq!(func_def.signature, "fun topLevelHelper(param: Int): Int");
    }

    #[test]
    fn test_parse_kotlin_references() {
        let parser = KotlinParser::new().unwrap();
        let code = r#"package com.example

import com.external.RemoteClient
import com.external.Config

fun runProcess(client: RemoteClient, cfg: Config) {
    client.connect(cfg.url)
    Logger.info("done")
}
"#;
        let tags = parser.parse(Path::new("src/run.kt"), code).unwrap();

        assert!(!tags.references.contains("runProcess"));
        assert!(tags.references.contains("RemoteClient"));
        assert!(tags.references.contains("Config"));
        assert!(tags.references.contains("client"));
        assert!(tags.references.contains("connect"));
        assert!(tags.references.contains("Logger"));
        assert!(tags.references.contains("info"));
    }

    #[test]
    fn test_parse_kotlin_malformed() {
        let parser = KotlinParser::new().unwrap();
        let code = r#"package com.example

class BrokenClass {
    fun broken( { error
}

class ValidKotlinClass {
    fun fine() {}
}
"#;
        let tags = parser.parse(Path::new("src/broken.kt"), code).unwrap();
        let valid = tags
            .definitions
            .iter()
            .find(|t| t.name == "ValidKotlinClass");
        assert!(valid.is_some());
        assert_eq!(valid.unwrap().kind, SymbolKind::Class);

        let fine = tags.definitions.iter().find(|t| t.name == "fine");
        assert!(fine.is_some());
        assert_eq!(fine.unwrap().kind, SymbolKind::Method);
    }
}
