use std::collections::HashSet;
use std::path::Path;

use super::LanguageParser;
use crate::{FileTags, RepomapError, Result, SymbolKind, Tag};

pub struct TypescriptParser {
    ts_language: tree_sitter::Language,
    tsx_language: tree_sitter::Language,
}

impl TypescriptParser {
    pub fn new() -> Result<Self> {
        let ts_language: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        let tsx_language: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TSX.into();
        Ok(Self {
            ts_language,
            tsx_language,
        })
    }
}

impl Default for TypescriptParser {
    fn default() -> Self {
        Self::new().expect("Failed to initialize TypescriptParser")
    }
}

impl LanguageParser for TypescriptParser {
    fn parse(&self, path: &Path, content: &str) -> Result<FileTags> {
        let is_tsx = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("tsx"))
            .unwrap_or(false);

        let lang = if is_tsx {
            &self.tsx_language
        } else {
            &self.ts_language
        };

        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(lang)
            .map_err(|e| RepomapError::Parser(e.to_string()))?;

        let tree = parser.parse(content, None).ok_or_else(|| {
            RepomapError::Parser("Failed to parse TypeScript source code".to_string())
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
        "interface_declaration" => Some(SymbolKind::Interface),
        "type_alias_declaration" => Some(SymbolKind::TypeAlias),
        "enum_declaration" => Some(SymbolKind::Enum),
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
    fn test_parse_typescript_definitions() {
        let parser = TypescriptParser::new().unwrap();
        let code = r#"
export interface User {
    id: string;
    name: string;
}

export type UserId = string | number;

export enum Status {
    Active = "ACTIVE",
    Inactive = "INACTIVE",
}

export class UserService implements User {
    id: string;
    name: string;

    constructor(id: string, name: string) {
        this.id = id;
        this.name = name;
    }

    public getUser(): User {
        return { id: this.id, name: this.name };
    }
}

export function createUserService(id: string, name: string): UserService {
    return new UserService(id, name);
}
"#;
        let tags = parser.parse(Path::new("service.ts"), code).unwrap();
        assert_eq!(tags.path, Path::new("service.ts"));

        let user_interface = tags.definitions.iter().find(|t| t.name == "User").unwrap();
        assert_eq!(user_interface.kind, SymbolKind::Interface);
        assert_eq!(user_interface.line, 2);
        assert_eq!(user_interface.signature, "export interface User");

        let user_id_type = tags
            .definitions
            .iter()
            .find(|t| t.name == "UserId")
            .unwrap();
        assert_eq!(user_id_type.kind, SymbolKind::TypeAlias);
        assert_eq!(user_id_type.line, 7);
        assert_eq!(
            user_id_type.signature,
            "export type UserId = string | number"
        );

        let status_enum = tags
            .definitions
            .iter()
            .find(|t| t.name == "Status")
            .unwrap();
        assert_eq!(status_enum.kind, SymbolKind::Enum);
        assert_eq!(status_enum.line, 9);
        assert_eq!(status_enum.signature, "export enum Status");

        let service_class = tags
            .definitions
            .iter()
            .find(|t| t.name == "UserService")
            .unwrap();
        assert_eq!(service_class.kind, SymbolKind::Class);
        assert_eq!(service_class.line, 14);
        assert_eq!(
            service_class.signature,
            "export class UserService implements User"
        );

        let get_user = tags
            .definitions
            .iter()
            .find(|t| t.name == "getUser")
            .unwrap();
        assert_eq!(get_user.kind, SymbolKind::Method);
        assert_eq!(get_user.line, 23);
        assert_eq!(get_user.signature, "public getUser(): User");

        let create_fn = tags
            .definitions
            .iter()
            .find(|t| t.name == "createUserService")
            .unwrap();
        assert_eq!(create_fn.kind, SymbolKind::Function);
        assert_eq!(create_fn.line, 28);
        assert_eq!(
            create_fn.signature,
            "export function createUserService(id: string, name: string): UserService"
        );
    }

    #[test]
    fn test_parse_typescript_tsx_components() {
        let parser = TypescriptParser::new().unwrap();
        let code = r#"
import React from 'react';

interface ButtonProps {
    label: string;
    onClick: () => void;
}

export function Button({ label, onClick }: ButtonProps) {
    return <button onClick={onClick}>{label}</button>;
}
"#;
        let tags = parser.parse(Path::new("Button.tsx"), code).unwrap();
        assert_eq!(tags.path, Path::new("Button.tsx"));

        let btn_props = tags
            .definitions
            .iter()
            .find(|t| t.name == "ButtonProps")
            .unwrap();
        assert_eq!(btn_props.kind, SymbolKind::Interface);

        let btn_fn = tags
            .definitions
            .iter()
            .find(|t| t.name == "Button")
            .unwrap();
        assert_eq!(btn_fn.kind, SymbolKind::Function);
        assert_eq!(btn_fn.line, 9);
        assert_eq!(
            btn_fn.signature,
            "export function Button({ label, onClick }: ButtonProps)"
        );

        // JSX tags / identifiers
        assert!(tags.references.contains("React"));
        assert!(!tags.references.contains("ButtonProps"));
        assert!(tags.references.contains("onClick"));
        assert!(!tags.references.contains("Button"));
    }

    #[test]
    fn test_parse_typescript_references() {
        let parser = TypescriptParser::new().unwrap();
        let code = r#"
import { Logger } from './logger';
import type { Config } from './config';

export function init(config: Config): void {
    const logger = new Logger(config);
    logger.log("initialized");
}
"#;
        let tags = parser.parse(Path::new("init.ts"), code).unwrap();
        assert_eq!(tags.definitions.len(), 1);
        assert_eq!(tags.definitions[0].name, "init");

        assert!(!tags.references.contains("init"));
        assert!(tags.references.contains("Logger"));
        assert!(tags.references.contains("Config"));
        assert!(tags.references.contains("config"));
        assert!(tags.references.contains("logger"));
    }

    #[test]
    fn test_parse_typescript_malformed() {
        let parser = TypescriptParser::new().unwrap();
        let code = r#"
interface Incomplete {
    prop: 

export function valid(n: number): number {
    return n + 1;
}
"#;
        let tags = parser.parse(Path::new("broken.ts"), code).unwrap();
        let valid = tags.definitions.iter().find(|t| t.name == "valid");
        assert!(valid.is_some());
        assert_eq!(valid.unwrap().kind, SymbolKind::Function);
    }
}
