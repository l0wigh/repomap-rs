use std::path::{Path, PathBuf};
use std::process::Command;

use repomap_rs::formatter::OutputFormat;
use repomap_rs::graph::{RankingStrategy, RepoGraph};
use repomap_rs::{budget, formatter, parser, scanner};

fn get_fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("sample_repo")
}

fn load_fixture_repo(root: &Path) -> (Vec<PathBuf>, Vec<repomap_rs::FileTags>) {
    let files = scanner::scan_repository(root).expect("scanner should scan fixture directory");
    let mut file_tags = Vec::new();
    for file in &files {
        let full_path = root.join(file);
        let content = std::fs::read_to_string(&full_path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {}", full_path.display(), e));
        let ft = parser::parse_file(file, &content)
            .unwrap_or_else(|e| panic!("Failed to parse {}: {}", file.display(), e))
            .unwrap_or_else(|| panic!("File not supported by parser: {}", file.display()));
        file_tags.push(ft);
    }
    (files, file_tags)
}

fn run_cli(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_repomap-rs"))
        .args(args)
        .output()
        .expect("Failed to execute repomap CLI");
    assert!(
        output.status.success(),
        "CLI should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("CLI output should be valid UTF-8")
}

#[test]
fn test_e2e_multi_language_parsing_and_filtering() {
    let fixture_root = get_fixture_root();
    let (files, file_tags) = load_fixture_repo(&fixture_root);

    // 1. Check file discovery and gitignore/extension filtering
    // Allowed fixture source files (12 languages):
    // AnalyticsTracker.kt, DatabaseService.java, OrderService.cs, app.ts, auth.py, controller.php,
    // engine.cpp, module.c, server.go, service.rs, utils.js, worker.rb
    // Excluded: doc.txt (unsupported extension), ignored.rs (.gitignore), build/ignored.rs (.gitignore)
    let expected_files = vec![
        PathBuf::from("AnalyticsTracker.kt"),
        PathBuf::from("DatabaseService.java"),
        PathBuf::from("OrderService.cs"),
        PathBuf::from("app.ts"),
        PathBuf::from("auth.py"),
        PathBuf::from("controller.php"),
        PathBuf::from("engine.cpp"),
        PathBuf::from("module.c"),
        PathBuf::from("server.go"),
        PathBuf::from("service.rs"),
        PathBuf::from("utils.js"),
        PathBuf::from("worker.rb"),
    ];
    assert_eq!(
        files, expected_files,
        "Discovered files must strictly match expected supported files"
    );

    // 2. Verify all 12 languages are parsed and tags extracted
    assert_eq!(file_tags.len(), 12);

    // TypeScript: app.ts defines AppController, references UserService
    let app_ts = file_tags
        .iter()
        .find(|ft| ft.path == Path::new("app.ts"))
        .expect("app.ts tags missing");
    assert!(
        app_ts.definitions.iter().any(|d| d.name == "AppController"),
        "AppController not found in app.ts"
    );
    assert!(
        app_ts.references.contains("UserService"),
        "app.ts should reference UserService"
    );

    // Python: auth.py defines AuthClient, references TokenManager
    let auth_py = file_tags
        .iter()
        .find(|ft| ft.path == Path::new("auth.py"))
        .expect("auth.py tags missing");
    assert!(
        auth_py.definitions.iter().any(|d| d.name == "AuthClient"),
        "AuthClient not found in auth.py"
    );
    assert!(
        auth_py.references.contains("TokenManager"),
        "auth.py should reference TokenManager"
    );

    // Go: server.go defines ServerHandler, references AppController
    let server_go = file_tags
        .iter()
        .find(|ft| ft.path == Path::new("server.go"))
        .expect("server.go tags missing");
    assert!(
        server_go
            .definitions
            .iter()
            .any(|d| d.name == "ServerHandler"),
        "ServerHandler not found in server.go"
    );
    assert!(
        server_go.references.contains("AppController"),
        "server.go should reference AppController"
    );

    // Rust: service.rs defines UserService, references AuthClient
    let service_rs = file_tags
        .iter()
        .find(|ft| ft.path == Path::new("service.rs"))
        .expect("service.rs tags missing");
    assert!(
        service_rs
            .definitions
            .iter()
            .any(|d| d.name == "UserService"),
        "UserService not found in service.rs"
    );
    assert!(
        service_rs.references.contains("AuthClient"),
        "service.rs should reference AuthClient"
    );

    // JavaScript: utils.js defines formatData
    let utils_js = file_tags
        .iter()
        .find(|ft| ft.path == Path::new("utils.js"))
        .expect("utils.js tags missing");
    assert!(
        utils_js.definitions.iter().any(|d| d.name == "formatData"),
        "formatData not found in utils.js"
    );

    // C: module.c defines c_module_init and CConfig, references ServerHandler
    let module_c = file_tags
        .iter()
        .find(|ft| ft.path == Path::new("module.c"))
        .expect("module.c tags missing");
    assert!(
        module_c
            .definitions
            .iter()
            .any(|d| d.name == "c_module_init"),
        "c_module_init not found in module.c"
    );
    assert!(
        module_c.definitions.iter().any(|d| d.name == "CConfig"),
        "CConfig not found in module.c"
    );
    assert!(
        module_c.references.contains("ServerHandler"),
        "module.c should reference ServerHandler"
    );

    // C++: engine.cpp defines RenderingEngine, start, and CoreEngine, references c_module_init
    let engine_cpp = file_tags
        .iter()
        .find(|ft| ft.path == Path::new("engine.cpp"))
        .expect("engine.cpp tags missing");
    assert!(
        engine_cpp
            .definitions
            .iter()
            .any(|d| d.name == "RenderingEngine"),
        "RenderingEngine not found in engine.cpp"
    );
    assert!(
        engine_cpp.definitions.iter().any(|d| d.name == "start"),
        "start not found in engine.cpp"
    );
    assert!(
        engine_cpp
            .definitions
            .iter()
            .any(|d| d.name == "CoreEngine"),
        "CoreEngine not found in engine.cpp"
    );
    assert!(
        engine_cpp.references.contains("c_module_init"),
        "engine.cpp should reference c_module_init"
    );

    // PHP: controller.php defines ApiController, references UserService
    let controller_php = file_tags
        .iter()
        .find(|ft| ft.path == Path::new("controller.php"))
        .expect("controller.php tags missing");
    assert!(
        controller_php
            .definitions
            .iter()
            .any(|d| d.name == "ApiController"),
        "ApiController not found in controller.php"
    );
    assert!(
        controller_php.references.contains("UserService"),
        "controller.php should reference UserService"
    );

    // Java: DatabaseService.java defines DatabaseService, references ApiController
    let db_java = file_tags
        .iter()
        .find(|ft| ft.path == Path::new("DatabaseService.java"))
        .expect("DatabaseService.java tags missing");
    assert!(
        db_java
            .definitions
            .iter()
            .any(|d| d.name == "DatabaseService"),
        "DatabaseService not found in DatabaseService.java"
    );
    assert!(
        db_java.references.contains("ApiController"),
        "DatabaseService.java should reference ApiController"
    );

    // C#: OrderService.cs defines OrderService, references DatabaseService
    let order_cs = file_tags
        .iter()
        .find(|ft| ft.path == Path::new("OrderService.cs"))
        .expect("OrderService.cs tags missing");
    assert!(
        order_cs
            .definitions
            .iter()
            .any(|d| d.name == "OrderService"),
        "OrderService not found in OrderService.cs"
    );
    assert!(
        order_cs.references.contains("DatabaseService"),
        "OrderService.cs should reference DatabaseService"
    );

    // Ruby: worker.rb defines BackgroundWorker, references OrderService
    let worker_rb = file_tags
        .iter()
        .find(|ft| ft.path == Path::new("worker.rb"))
        .expect("worker.rb tags missing");
    assert!(
        worker_rb
            .definitions
            .iter()
            .any(|d| d.name == "BackgroundWorker"),
        "BackgroundWorker not found in worker.rb"
    );
    assert!(
        worker_rb.references.contains("OrderService"),
        "worker.rb should reference OrderService"
    );

    // Kotlin: AnalyticsTracker.kt defines AnalyticsTracker, references BackgroundWorker
    let analytics_kt = file_tags
        .iter()
        .find(|ft| ft.path == Path::new("AnalyticsTracker.kt"))
        .expect("AnalyticsTracker.kt tags missing");
    assert!(
        analytics_kt
            .definitions
            .iter()
            .any(|d| d.name == "AnalyticsTracker"),
        "AnalyticsTracker not found in AnalyticsTracker.kt"
    );
    assert!(
        analytics_kt.references.contains("BackgroundWorker"),
        "AnalyticsTracker.kt should reference BackgroundWorker"
    );
}

#[test]
fn test_e2e_personalized_pagerank_focus() {
    let fixture_root = get_fixture_root();
    let (_files, file_tags) = load_fixture_repo(&fixture_root);
    let graph = RepoGraph::from_file_tags(&file_tags);

    // Graph dependency chains:
    // AnalyticsTracker.kt -> worker.rb (BackgroundWorker) -> OrderService.cs (OrderService) -> DatabaseService.java (DatabaseService) -> controller.php (ApiController) -> service.rs (UserService) -> auth.py (AuthClient)
    // engine.cpp -> module.c (c_module_init) -> server.go (ServerHandler) -> app.ts (AppController) -> service.rs (UserService) -> auth.py (AuthClient)
    // utils.js is disconnected.
    //
    // Case 1: Focusing on auth.py
    // auth.py has teleport mass 1.0.
    // Disconnected utils.js receives 0 teleport and has no incoming edges, so its score is 0.
    let focus_auth = vec![PathBuf::from("auth.py")];
    let ranked_ppr = graph.compute_pagerank(&focus_auth);

    let auth_rank = ranked_ppr
        .iter()
        .position(|f| f.path == Path::new("auth.py"))
        .expect("auth.py should be in ranked files");
    let utils_file = ranked_ppr
        .iter()
        .find(|f| f.path == Path::new("utils.js"))
        .expect("utils.js should be in ranked files");

    // auth.py should be ranked top or higher than disconnected utils.js
    assert_eq!(
        auth_rank, 0,
        "auth.py should be top ranked when focused on it"
    );
    assert!(
        ranked_ppr[0].score > utils_file.score,
        "Focused auth.py score ({}) must be higher than disconnected utils.js score ({})",
        ranked_ppr[0].score,
        utils_file.score
    );
    assert!(
        utils_file.score < 1e-6,
        "Disconnected utils.js should have near zero score under PPR focused on auth.py"
    );

    // Case 2: Uniform PageRank (no focus)
    // In uniform PageRank, incoming edges to auth.py (from service.rs) and service.rs (from app.ts)
    // should give auth.py higher score than disconnected utils.js.
    let ranked_uniform = graph.compute_pagerank(&[]);
    let auth_score_uniform = ranked_uniform
        .iter()
        .find(|f| f.path == Path::new("auth.py"))
        .unwrap()
        .score;
    let utils_score_uniform = ranked_uniform
        .iter()
        .find(|f| f.path == Path::new("utils.js"))
        .unwrap()
        .score;
    assert!(
        auth_score_uniform > utils_score_uniform,
        "auth.py score in uniform PageRank ({}) should exceed unreferenced utils.js ({})",
        auth_score_uniform,
        utils_score_uniform
    );
}

#[test]
fn test_e2e_strict_token_budget() {
    let fixture_root = get_fixture_root();
    let (_files, file_tags) = load_fixture_repo(&fixture_root);
    let graph = RepoGraph::from_file_tags(&file_tags);
    let ranked = graph.compute_pagerank(&[]);

    let test_budgets = [30, 80, 200, 1000];

    for budget in test_budgets {
        for encoding in ["cl100k_base", "o200k_base"] {
            let plan = budget::fit_to_budget(&ranked, budget, encoding)
                .unwrap_or_else(|e| panic!("fit_to_budget failed for budget {budget}: {e}"));

            assert!(
                plan.total_tokens <= budget,
                "Budget violation for budget {} ({}) : total_tokens {} > max_tokens {}",
                budget,
                encoding,
                plan.total_tokens,
                budget
            );

            // Format as Aider and verify token count with tiktoken
            let aider_text = formatter::format_repomap(&plan, OutputFormat::Aider)
                .expect("Aider format should succeed");
            let actual_tokens = budget::count_tokens(&aider_text, encoding)
                .expect("Token count should succeed on generated aider text");
            assert_eq!(
                plan.total_tokens, actual_tokens,
                "Plan total_tokens must match actual token count of rendered text"
            );
            assert!(
                actual_tokens <= budget,
                "Rendered Aider map exceeded budget: {} > {}",
                actual_tokens,
                budget
            );
        }
    }
}

#[test]
fn test_e2e_formatters_aider_and_json() {
    let fixture_root = get_fixture_root();
    let (_files, file_tags) = load_fixture_repo(&fixture_root);
    let graph = RepoGraph::from_file_tags(&file_tags);
    let ranked = graph.compute_pagerank(&[]);
    let plan = budget::fit_to_budget(&ranked, 500, "cl100k_base").expect("fit_to_budget");

    assert!(
        !plan.files.is_empty(),
        "Plan should contain files for 500 tokens"
    );

    // 1. Aider formatting
    let aider_output = formatter::format_repomap(&plan, OutputFormat::Aider).expect("aider format");
    assert!(
        aider_output.contains("│⋮..."),
        "Aider output should contain ellipsis markers '│⋮...'"
    );
    for file in &plan.files {
        let path_str = file.path.to_string_lossy();
        assert!(
            aider_output.contains(&format!("{}:", path_str)),
            "Aider output must contain file header for {}",
            path_str
        );
        for def in &file.definitions {
            assert!(
                aider_output.contains(&format!("│{}", def.signature)),
                "Aider output must contain symbol signature: {}",
                def.signature
            );
        }
    }

    // 2. JSON formatting
    let json_output = formatter::format_repomap(&plan, OutputFormat::Json).expect("json format");
    let parsed: serde_json::Value =
        serde_json::from_str(&json_output).expect("JSON output must be valid syntax");

    assert_eq!(parsed["total_tokens"], plan.total_tokens);
    assert_eq!(parsed["max_tokens"], plan.max_tokens);
    let files_json = parsed["files"].as_array().expect("files must be an array");
    assert_eq!(files_json.len(), plan.files.len());

    for (file_idx, file) in plan.files.iter().enumerate() {
        let f_obj = &files_json[file_idx];
        assert_eq!(f_obj["path"], file.path.to_string_lossy().as_ref());
        let defs_json = f_obj["definitions"]
            .as_array()
            .expect("definitions must be an array");
        assert_eq!(defs_json.len(), file.definitions.len());

        for (d_idx, d) in file.definitions.iter().enumerate() {
            assert_eq!(defs_json[d_idx]["name"], d.name);
            assert_eq!(defs_json[d_idx]["line"], d.line);
            assert_eq!(defs_json[d_idx]["signature"], d.signature);
        }
    }
}

#[test]
fn test_e2e_determinism() {
    let fixture_root = get_fixture_root();

    // Run pipeline run 1
    let (files1, tags1) = load_fixture_repo(&fixture_root);
    let graph1 = RepoGraph::from_file_tags(&tags1);
    let ranked1 = graph1.compute_pagerank(&[PathBuf::from("service.rs")]);
    let plan1 = budget::fit_to_budget(&ranked1, 600, "cl100k_base").unwrap();
    let aider1 = formatter::format_repomap(&plan1, OutputFormat::Aider).unwrap();
    let json1 = formatter::format_repomap(&plan1, OutputFormat::Json).unwrap();

    // Run pipeline run 2
    let (files2, tags2) = load_fixture_repo(&fixture_root);
    let graph2 = RepoGraph::from_file_tags(&tags2);
    let ranked2 = graph2.compute_pagerank(&[PathBuf::from("service.rs")]);
    let plan2 = budget::fit_to_budget(&ranked2, 600, "cl100k_base").unwrap();
    let aider2 = formatter::format_repomap(&plan2, OutputFormat::Aider).unwrap();
    let json2 = formatter::format_repomap(&plan2, OutputFormat::Json).unwrap();

    // Assert bit-for-bit identity across runs
    assert_eq!(files1, files2, "Scanned files must be identical");
    assert_eq!(tags1, tags2, "Extracted tags must be identical");
    assert_eq!(ranked1, ranked2, "Ranked files must be identical");
    assert_eq!(plan1, plan2, "Budget plans must be identical");
    assert_eq!(aider1, aider2, "Aider text output must be identical");
    assert_eq!(json1, json2, "JSON output must be identical");
}

#[test]
fn test_e2e_cli_ranking_selection_default_and_legacy() {
    let fixture_root = get_fixture_root();
    let fixture_root_str = fixture_root.to_string_lossy().to_string();

    let default_json_text = run_cli(&[
        fixture_root_str.as_str(),
        "--format",
        "json",
        "--max-tokens",
        "500",
    ]);
    let balanced_json_text = run_cli(&[
        fixture_root_str.as_str(),
        "--format",
        "json",
        "--max-tokens",
        "500",
        "--ranking",
        "balanced",
    ]);
    let legacy_json_text = run_cli(&[
        fixture_root_str.as_str(),
        "--format",
        "json",
        "--max-tokens",
        "500",
        "--ranking",
        "legacy",
    ]);

    let default_json: serde_json::Value =
        serde_json::from_str(&default_json_text).expect("default output should be valid JSON");
    let balanced_json: serde_json::Value =
        serde_json::from_str(&balanced_json_text).expect("balanced output should be valid JSON");
    let legacy_json: serde_json::Value =
        serde_json::from_str(&legacy_json_text).expect("legacy output should be valid JSON");

    assert_eq!(
        default_json, balanced_json,
        "Default CLI ranking should match explicit balanced ranking"
    );

    let (_files, file_tags) = load_fixture_repo(&fixture_root);
    let graph = RepoGraph::from_file_tags(&file_tags);
    let ranked_balanced = graph.compute_ranked_files(&[], RankingStrategy::Balanced);
    let ranked_legacy = graph.compute_ranked_files(&[], RankingStrategy::Legacy);
    let plan_balanced =
        budget::fit_to_budget(&ranked_balanced, 500, "cl100k_base").expect("balanced plan");
    let plan_legacy =
        budget::fit_to_budget(&ranked_legacy, 500, "cl100k_base").expect("legacy plan");

    let expected_balanced = formatter::format_repomap(&plan_balanced, OutputFormat::Json)
        .expect("format balanced json");
    let expected_legacy =
        formatter::format_repomap(&plan_legacy, OutputFormat::Json).expect("format legacy json");

    let expected_balanced_json: serde_json::Value =
        serde_json::from_str(&expected_balanced).expect("expected balanced JSON");
    let expected_legacy_json: serde_json::Value =
        serde_json::from_str(&expected_legacy).expect("expected legacy JSON");

    assert_eq!(
        default_json, expected_balanced_json,
        "CLI default output should match library balanced ranking output"
    );
    assert_eq!(
        legacy_json, expected_legacy_json,
        "CLI legacy output should match library legacy ranking output"
    );
}
