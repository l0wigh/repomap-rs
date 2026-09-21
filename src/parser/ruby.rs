use std::collections::HashSet;
use std::path::Path;

use super::LanguageParser;
use crate::{FileTags, RepomapError, Result, SymbolKind, Tag};

pub struct RubyParser {
    language: tree_sitter::Language,
}

impl RubyParser {
    pub fn new() -> Result<Self> {
        let language: tree_sitter::Language = tree_sitter_ruby::LANGUAGE.into();
        Ok(Self { language })
    }
}

impl Default for RubyParser {
    fn default() -> Self {
        Self::new().expect("Failed to initialize RubyParser")
    }
}

impl LanguageParser for RubyParser {
    fn parse(&self, path: &Path, content: &str) -> Result<FileTags> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&self.language)
            .map_err(|e| RepomapError::Parser(e.to_string()))?;

        let tree = parser
            .parse(content, None)
            .ok_or_else(|| RepomapError::Parser("Failed to parse Ruby source code".to_string()))?;

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
    let first_line = text.lines().next().unwrap_or("").trim();
    first_line.to_string()
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
        "class" => Some(SymbolKind::Class),
        "module" => Some(SymbolKind::Class),
        "method" | "singleton_method" => Some(SymbolKind::Method),
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

    if (node.kind() == "identifier" || node.kind() == "constant")
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
    fn test_parse_ruby_definitions() {
        let parser = RubyParser::new().unwrap();
        let code = r#"module Analytics
  class EventTracker < BaseTracker
    def track(event_name, payload = {})
      process(event_name)
    end

    def self.default_tracker
      new
    end
  end
end
"#;
        let tags = parser.parse(Path::new("src/tracker.rb"), code).unwrap();
        assert_eq!(tags.path, Path::new("src/tracker.rb"));

        let mod_def = tags
            .definitions
            .iter()
            .find(|t| t.name == "Analytics")
            .expect("Analytics module missing");
        assert_eq!(mod_def.kind, SymbolKind::Class);
        assert_eq!(mod_def.line, 1);
        assert_eq!(mod_def.signature, "module Analytics");

        let class_def = tags
            .definitions
            .iter()
            .find(|t| t.name == "EventTracker")
            .expect("EventTracker class missing");
        assert_eq!(class_def.kind, SymbolKind::Class);
        assert_eq!(class_def.line, 2);
        assert_eq!(class_def.signature, "class EventTracker < BaseTracker");

        let instance_method = tags
            .definitions
            .iter()
            .find(|t| t.name == "track")
            .expect("track method missing");
        assert_eq!(instance_method.kind, SymbolKind::Method);
        assert_eq!(instance_method.line, 3);
        assert_eq!(
            instance_method.signature,
            "def track(event_name, payload = {})"
        );

        let singleton_method = tags
            .definitions
            .iter()
            .find(|t| t.name == "default_tracker")
            .expect("default_tracker method missing");
        assert_eq!(singleton_method.kind, SymbolKind::Method);
        assert_eq!(singleton_method.line, 7);
        assert_eq!(singleton_method.signature, "def self.default_tracker");
    }

    #[test]
    fn test_parse_ruby_references() {
        let parser = RubyParser::new().unwrap();
        let code = r#"class Service
  def perform
    client = ExternalClient.new
    client.post_data(Config::ENDPOINT)
  end
end
"#;
        let tags = parser.parse(Path::new("src/service.rb"), code).unwrap();

        assert!(!tags.references.contains("Service"));
        assert!(!tags.references.contains("perform"));
        assert!(tags.references.contains("ExternalClient"));
        assert!(tags.references.contains("new"));
        assert!(tags.references.contains("client"));
        assert!(tags.references.contains("post_data"));
        assert!(tags.references.contains("Config"));
        assert!(tags.references.contains("ENDPOINT"));
    }

    #[test]
    fn test_parse_ruby_malformed() {
        let parser = RubyParser::new().unwrap();
        let code = r#"class Broken
  def bad_method(
end

class Working
  def valid_method
    42
  end
end
"#;
        let tags = parser.parse(Path::new("src/broken.rb"), code).unwrap();
        let working = tags.definitions.iter().find(|t| t.name == "Working");
        assert!(working.is_some());
        assert_eq!(working.unwrap().kind, SymbolKind::Class);

        let valid = tags.definitions.iter().find(|t| t.name == "valid_method");
        assert!(valid.is_some());
        assert_eq!(valid.unwrap().kind, SymbolKind::Method);
    }
}
