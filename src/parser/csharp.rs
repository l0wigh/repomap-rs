use std::collections::HashSet;
use std::path::Path;

use super::LanguageParser;
use crate::{FileTags, RepomapError, Result, SymbolKind, Tag};

pub struct CsharpParser {
    language: tree_sitter::Language,
}

impl CsharpParser {
    pub fn new() -> Result<Self> {
        let language: tree_sitter::Language = tree_sitter_c_sharp::LANGUAGE.into();
        Ok(Self { language })
    }
}

impl Default for CsharpParser {
    fn default() -> Self {
        Self::new().expect("Failed to initialize CsharpParser")
    }
}

impl LanguageParser for CsharpParser {
    fn parse(&self, path: &Path, content: &str) -> Result<FileTags> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&self.language)
            .map_err(|e| RepomapError::Parser(e.to_string()))?;

        let tree = parser
            .parse(content, None)
            .ok_or_else(|| RepomapError::Parser("Failed to parse C# source code".to_string()))?;

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
                && !l.starts_with('[')
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
        "record_struct_declaration" | "struct_declaration" => Some(SymbolKind::Struct),
        "enum_declaration" => Some(SymbolKind::Enum),
        "namespace_declaration" | "file_scoped_namespace_declaration" => Some(SymbolKind::Class),
        "method_declaration" | "constructor_declaration" => Some(SymbolKind::Method),
        "local_function_statement" => Some(SymbolKind::Function),
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
    fn test_parse_csharp_definitions() {
        let parser = CsharpParser::new().unwrap();
        let code = r#"namespace Company.Product
{
    public interface IProcessor
    {
        void Process();
    }

    public struct Vector2
    {
        public float X;
        public float Y;
    }

    public record UserRecord(string Username, int Id);

    public class DataProcessor : IProcessor
    {
        public DataProcessor() {}

        public void Process()
        {
            void LocalHelper() {}
            LocalHelper();
        }
    }
}
"#;
        let tags = parser
            .parse(Path::new("src/DataProcessor.cs"), code)
            .unwrap();
        assert_eq!(tags.path, Path::new("src/DataProcessor.cs"));

        let ns = tags
            .definitions
            .iter()
            .find(|t| t.name == "Company.Product")
            .expect("Namespace missing");
        assert_eq!(ns.kind, SymbolKind::Class);
        assert_eq!(ns.line, 1);
        assert_eq!(ns.signature, "namespace Company.Product");

        let iface = tags
            .definitions
            .iter()
            .find(|t| t.name == "IProcessor")
            .expect("IProcessor missing");
        assert_eq!(iface.kind, SymbolKind::Interface);
        assert_eq!(iface.line, 3);
        assert_eq!(iface.signature, "public interface IProcessor");

        let iface_method = tags
            .definitions
            .iter()
            .find(|t| t.name == "Process" && t.line == 5)
            .expect("Process interface method missing");
        assert_eq!(iface_method.kind, SymbolKind::Method);
        assert_eq!(iface_method.line, 5);
        assert_eq!(iface_method.signature, "void Process()");

        let struct_def = tags
            .definitions
            .iter()
            .find(|t| t.name == "Vector2")
            .expect("Vector2 struct missing");
        assert_eq!(struct_def.kind, SymbolKind::Struct);
        assert_eq!(struct_def.line, 8);
        assert_eq!(struct_def.signature, "public struct Vector2");

        let record_def = tags
            .definitions
            .iter()
            .find(|t| t.name == "UserRecord")
            .expect("UserRecord missing");
        assert_eq!(record_def.kind, SymbolKind::Class);
        assert_eq!(record_def.line, 14);
        assert_eq!(
            record_def.signature,
            "public record UserRecord(string Username, int Id)"
        );

        let class_def = tags
            .definitions
            .iter()
            .find(|t| t.name == "DataProcessor")
            .expect("DataProcessor missing");
        assert_eq!(class_def.kind, SymbolKind::Class);
        assert_eq!(class_def.line, 16);
        assert_eq!(
            class_def.signature,
            "public class DataProcessor : IProcessor"
        );

        let ctor = tags
            .definitions
            .iter()
            .find(|t| t.name == "DataProcessor" && t.line == 18)
            .expect("DataProcessor ctor missing");
        assert_eq!(ctor.kind, SymbolKind::Method);
        assert_eq!(ctor.line, 18);
        assert_eq!(ctor.signature, "public DataProcessor()");

        let class_method = tags
            .definitions
            .iter()
            .find(|t| t.name == "Process" && t.line == 20)
            .expect("Process method missing");
        assert_eq!(class_method.kind, SymbolKind::Method);
        assert_eq!(class_method.line, 20);
        assert_eq!(class_method.signature, "public void Process()");

        let local_fn = tags
            .definitions
            .iter()
            .find(|t| t.name == "LocalHelper")
            .expect("LocalHelper function missing");
        assert_eq!(local_fn.kind, SymbolKind::Function);
        assert_eq!(local_fn.line, 22);
        assert_eq!(local_fn.signature, "void LocalHelper()");
    }

    #[test]
    fn test_parse_csharp_references() {
        let parser = CsharpParser::new().unwrap();
        let code = r#"namespace App
{
    public class Consumer
    {
        public void Run(IExternalService service, ConfigOptions options)
        {
            service.Initialize(options);
            Logger.Log("Started");
        }
    }
}
"#;
        let tags = parser.parse(Path::new("src/Consumer.cs"), code).unwrap();

        assert!(!tags.references.contains("Consumer"));
        assert!(!tags.references.contains("Run"));
        assert!(tags.references.contains("IExternalService"));
        assert!(tags.references.contains("ConfigOptions"));
        assert!(tags.references.contains("service"));
        assert!(tags.references.contains("Initialize"));
        assert!(tags.references.contains("Logger"));
        assert!(tags.references.contains("Log"));
    }

    #[test]
    fn test_parse_csharp_malformed() {
        let parser = CsharpParser::new().unwrap();
        let code = r#"namespace Broken
{
    public class Faulty {
        syntax error $$$$
    }

    public class Intact {
        public void GoodMethod() {}
    }
}
"#;
        let tags = parser.parse(Path::new("src/Broken.cs"), code).unwrap();
        let intact = tags.definitions.iter().find(|t| t.name == "Intact");
        assert!(intact.is_some());
        assert_eq!(intact.unwrap().kind, SymbolKind::Class);

        let good = tags.definitions.iter().find(|t| t.name == "GoodMethod");
        assert!(good.is_some());
        assert_eq!(good.unwrap().kind, SymbolKind::Method);
    }
}
