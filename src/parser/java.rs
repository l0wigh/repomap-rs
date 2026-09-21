use std::collections::HashSet;
use std::path::Path;

use super::LanguageParser;
use crate::{FileTags, RepomapError, Result, SymbolKind, Tag};

pub struct JavaParser {
    language: tree_sitter::Language,
}

impl JavaParser {
    pub fn new() -> Result<Self> {
        let language: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
        Ok(Self { language })
    }
}

impl Default for JavaParser {
    fn default() -> Self {
        Self::new().expect("Failed to initialize JavaParser")
    }
}

impl LanguageParser for JavaParser {
    fn parse(&self, path: &Path, content: &str) -> Result<FileTags> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&self.language)
            .map_err(|e| RepomapError::Parser(e.to_string()))?;

        let tree = parser
            .parse(content, None)
            .ok_or_else(|| RepomapError::Parser("Failed to parse Java source code".to_string()))?;

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
        "record_declaration" => Some(SymbolKind::Class),
        "enum_declaration" => Some(SymbolKind::Enum),
        "method_declaration" | "constructor_declaration" => Some(SymbolKind::Method),
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

    if (node.kind() == "identifier" || node.kind() == "type_identifier")
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
    fn test_parse_java_definitions() {
        let parser = JavaParser::new().unwrap();
        let code = r#"package com.example;

public interface Repository<T> {
    T findById(Long id);
}

public enum Priority {
    LOW, HIGH
}

public record Point(int x, int y) {}

public class UserRepository implements Repository<User> {
    private String name;

    public UserRepository(String name) {
        this.name = name;
    }

    @Override
    public User findById(Long id) {
        return null;
    }
}
"#;
        let tags = parser
            .parse(Path::new("src/Repository.java"), code)
            .unwrap();
        assert_eq!(tags.path, Path::new("src/Repository.java"));

        let iface = tags
            .definitions
            .iter()
            .find(|t| t.name == "Repository")
            .expect("Repository interface missing");
        assert_eq!(iface.kind, SymbolKind::Interface);
        assert_eq!(iface.line, 3);
        assert_eq!(iface.signature, "public interface Repository<T>");

        let iface_method = tags
            .definitions
            .iter()
            .find(|t| t.name == "findById" && t.line == 4)
            .expect("findById interface method missing");
        assert_eq!(iface_method.kind, SymbolKind::Method);
        assert_eq!(iface_method.line, 4);
        assert_eq!(iface_method.signature, "T findById(Long id)");

        let enum_def = tags
            .definitions
            .iter()
            .find(|t| t.name == "Priority")
            .expect("Priority enum missing");
        assert_eq!(enum_def.kind, SymbolKind::Enum);
        assert_eq!(enum_def.line, 7);
        assert_eq!(enum_def.signature, "public enum Priority");

        let record_def = tags
            .definitions
            .iter()
            .find(|t| t.name == "Point")
            .expect("Point record missing");
        assert_eq!(record_def.kind, SymbolKind::Class);
        assert_eq!(record_def.line, 11);
        assert_eq!(record_def.signature, "public record Point(int x, int y)");

        let class_def = tags
            .definitions
            .iter()
            .find(|t| t.name == "UserRepository")
            .expect("UserRepository class missing");
        assert_eq!(class_def.kind, SymbolKind::Class);
        assert_eq!(class_def.line, 13);
        assert_eq!(
            class_def.signature,
            "public class UserRepository implements Repository<User>"
        );

        let ctor = tags
            .definitions
            .iter()
            .find(|t| t.name == "UserRepository" && t.line == 16)
            .expect("Constructor missing");
        assert_eq!(ctor.kind, SymbolKind::Method);
        assert_eq!(ctor.line, 16);
        assert_eq!(ctor.signature, "public UserRepository(String name)");

        let class_method = tags
            .definitions
            .iter()
            .find(|t| t.name == "findById" && t.line == 21)
            .expect("findById class method missing");
        assert_eq!(class_method.kind, SymbolKind::Method);
        assert_eq!(class_method.line, 21);
        assert_eq!(class_method.signature, "public User findById(Long id)");
    }

    #[test]
    fn test_parse_java_references() {
        let parser = JavaParser::new().unwrap();
        let code = r#"package com.example;

import java.util.List;
import com.example.service.AuthService;

public class Client {
    public void execute(AuthService service, List<String> items) {
        service.authenticate();
        Helper.run();
    }
}
"#;
        let tags = parser.parse(Path::new("src/Client.java"), code).unwrap();

        assert!(!tags.references.contains("Client"));
        assert!(!tags.references.contains("execute"));
        assert!(tags.references.contains("AuthService"));
        assert!(tags.references.contains("List"));
        assert!(tags.references.contains("String"));
        assert!(tags.references.contains("authenticate"));
        assert!(tags.references.contains("Helper"));
        assert!(tags.references.contains("run"));
    }

    #[test]
    fn test_parse_java_malformed() {
        let parser = JavaParser::new().unwrap();
        let code = r#"package com.example;

public class Broken {
    syntax error !!! {{{{
}

public class ValidJavaClass {
    public void work() {}
}
"#;
        let tags = parser.parse(Path::new("src/Broken.java"), code).unwrap();
        let valid = tags.definitions.iter().find(|t| t.name == "ValidJavaClass");
        assert!(valid.is_some());
        assert_eq!(valid.unwrap().kind, SymbolKind::Class);

        let work = tags.definitions.iter().find(|t| t.name == "work");
        assert!(work.is_some());
        assert_eq!(work.unwrap().kind, SymbolKind::Method);
    }
}
