use std::collections::HashSet;
use std::path::Path;

use super::LanguageParser;
use crate::{FileTags, RepomapError, Result, SymbolKind, Tag};

pub struct JavascriptParser {
    language: tree_sitter::Language,
}

impl JavascriptParser {
    pub fn new() -> Result<Self> {
        let language: tree_sitter::Language = tree_sitter_javascript::LANGUAGE.into();
        Ok(Self { language })
    }
}

impl Default for JavascriptParser {
    fn default() -> Self {
        Self::new().expect("Failed to initialize JavascriptParser")
    }
}

impl LanguageParser for JavascriptParser {
    fn parse(&self, path: &Path, content: &str) -> Result<FileTags> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&self.language)
            .map_err(|e| RepomapError::Parser(e.to_string()))?;

        let tree = parser.parse(content, None).ok_or_else(|| {
            RepomapError::Parser("Failed to parse JavaScript source code".to_string())
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
    let start = if let Some(parent) = node.parent() {
        if parent.kind() == "export_statement" {
            parent.start_byte()
        } else {
            node.start_byte()
        }
    } else {
        node.start_byte()
    };

    let text = if let Some(body) = node.child_by_field_name("body") {
        if body.start_byte() >= start && body.start_byte() <= content.len() {
            &content[start..body.start_byte()]
        } else {
            &content[start..node.end_byte()]
        }
    } else {
        &content[start..node.end_byte()]
    };

    let cut = if let Some(idx) = text.find(['{', ';']) {
        if node.child_by_field_name("body").is_some() {
            text
        } else {
            &text[..idx]
        }
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
        "class_declaration" => Some(SymbolKind::Class),
        "function_declaration" => Some(SymbolKind::Function),
        "method_definition" => Some(SymbolKind::Method),
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
    fn test_parse_javascript_classes_and_functions() {
        let parser = JavascriptParser::new().unwrap();
        let code = r#"
class Animal {
    constructor(name) {
        this.name = name;
    }

    speak() {
        console.log(this.name);
    }
}

function createAnimal(name) {
    return new Animal(name);
}

export class Dog extends Animal {
    bark() {
        return "woof";
    }
}

export default function makeNoise() {
    return "noise";
}
"#;
        let tags = parser.parse(Path::new("animal.js"), code).unwrap();
        assert_eq!(tags.path, Path::new("animal.js"));

        let animal = tags
            .definitions
            .iter()
            .find(|t| t.name == "Animal")
            .unwrap();
        assert_eq!(animal.kind, SymbolKind::Class);
        assert_eq!(animal.line, 2);
        assert_eq!(animal.signature, "class Animal");

        let speak = tags.definitions.iter().find(|t| t.name == "speak").unwrap();
        assert_eq!(speak.kind, SymbolKind::Method);
        assert_eq!(speak.line, 7);
        assert_eq!(speak.signature, "speak()");

        let create = tags
            .definitions
            .iter()
            .find(|t| t.name == "createAnimal")
            .unwrap();
        assert_eq!(create.kind, SymbolKind::Function);
        assert_eq!(create.line, 12);
        assert_eq!(create.signature, "function createAnimal(name)");

        let dog = tags.definitions.iter().find(|t| t.name == "Dog").unwrap();
        assert_eq!(dog.kind, SymbolKind::Class);
        assert_eq!(dog.line, 16);
        assert_eq!(dog.signature, "export class Dog extends Animal");

        let bark = tags.definitions.iter().find(|t| t.name == "bark").unwrap();
        assert_eq!(bark.kind, SymbolKind::Method);
        assert_eq!(bark.line, 17);
        assert_eq!(bark.signature, "bark()");

        let noise = tags
            .definitions
            .iter()
            .find(|t| t.name == "makeNoise")
            .unwrap();
        assert_eq!(noise.kind, SymbolKind::Function);
        assert_eq!(noise.line, 22);
        assert_eq!(noise.signature, "export default function makeNoise()");
    }

    #[test]
    fn test_parse_javascript_references() {
        let parser = JavascriptParser::new().unwrap();
        let code = r#"
import { helper } from './utils';

function process(data) {
    const result = helper(data);
    return externalService.send(result);
}
"#;
        let tags = parser.parse(Path::new("process.js"), code).unwrap();
        assert_eq!(tags.definitions.len(), 1);
        assert_eq!(tags.definitions[0].name, "process");
        assert!(!tags.references.contains("process"));

        assert!(tags.references.contains("helper"));
        assert!(tags.references.contains("data"));
        assert!(tags.references.contains("result"));
        assert!(tags.references.contains("externalService"));
    }

    #[test]
    fn test_parse_javascript_malformed() {
        let parser = JavascriptParser::new().unwrap();
        let code = r#"
class Broken {
    constructor(
}

function validFunc() {
    return 42;
}
"#;
        let tags = parser.parse(Path::new("broken.js"), code).unwrap();
        let valid = tags.definitions.iter().find(|t| t.name == "validFunc");
        assert!(valid.is_some());
        assert_eq!(valid.unwrap().kind, SymbolKind::Function);
    }
}
