use std::path::Path;

#[allow(unused_imports)]
use crate::{FileTags, Result, SymbolKind, Tag};

pub trait LanguageParser: Send + Sync {
    fn parse(&self, path: &Path, content: &str) -> Result<FileTags>;
}

pub mod go;
pub mod javascript;
pub mod python;
pub mod rust;
pub mod typescript;

pub fn parse_file(path: &Path, content: &str) -> Result<Option<FileTags>> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "rs" => {
            let parser = rust::RustParser::new()?;
            Ok(Some(parser.parse(path, content)?))
        }
        "py" => {
            let parser = python::PythonParser::new()?;
            Ok(Some(parser.parse(path, content)?))
        }
        "js" | "jsx" | "mjs" | "cjs" => {
            let parser = javascript::JavascriptParser::new()?;
            Ok(Some(parser.parse(path, content)?))
        }
        "ts" | "tsx" | "mts" | "cts" => {
            let parser = typescript::TypescriptParser::new()?;
            Ok(Some(parser.parse(path, content)?))
        }
        "go" => {
            let parser = go::GoParser::new()?;
            Ok(Some(parser.parse(path, content)?))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_file_dispatcher() {
        let code = "pub fn foo() {}";
        let res = parse_file(Path::new("src/lib.rs"), code).unwrap();
        assert!(res.is_some());
        let tags = res.unwrap();
        assert_eq!(tags.definitions.len(), 1);
        assert_eq!(tags.definitions[0].name, "foo");

        let py_res = parse_file(Path::new("src/lib.py"), "def foo(): pass").unwrap();
        assert!(py_res.is_some());
        let py_tags = py_res.unwrap();
        assert_eq!(py_tags.definitions.len(), 1);
        assert_eq!(py_tags.definitions[0].name, "foo");

        let js_res = parse_file(Path::new("src/index.js"), "function foo() {}").unwrap();
        assert!(js_res.is_some());
        let js_tags = js_res.unwrap();
        assert_eq!(js_tags.definitions.len(), 1);
        assert_eq!(js_tags.definitions[0].name, "foo");

        let ts_res = parse_file(Path::new("src/index.ts"), "function foo(): void {}").unwrap();
        assert!(ts_res.is_some());
        let ts_tags = ts_res.unwrap();
        assert_eq!(ts_tags.definitions.len(), 1);
        assert_eq!(ts_tags.definitions[0].name, "foo");

        let tsx_res = parse_file(
            Path::new("src/App.tsx"),
            "function App() { return <div/>; }",
        )
        .unwrap();
        assert!(tsx_res.is_some());
        let tsx_tags = tsx_res.unwrap();
        assert_eq!(tsx_tags.definitions.len(), 1);
        assert_eq!(tsx_tags.definitions[0].name, "App");

        let go_res = parse_file(Path::new("src/main.go"), "func main() {}").unwrap();
        assert!(go_res.is_some());
        let go_tags = go_res.unwrap();
        assert_eq!(go_tags.definitions.len(), 1);
        assert_eq!(go_tags.definitions[0].name, "main");

        let non_supported = parse_file(Path::new("src/lib.txt"), "hello").unwrap();
        assert!(non_supported.is_none());
    }
}
