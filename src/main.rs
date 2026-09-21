use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use repomap_rs::formatter::OutputFormat;
use repomap_rs::graph::RepoGraph;
use repomap_rs::{Result, budget, formatter, parser, scanner};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliOutputFormat {
    #[value(name = "aider")]
    Aider,
    #[value(name = "json")]
    Json,
}

impl std::fmt::Display for CliOutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliOutputFormat::Aider => write!(f, "aider"),
            CliOutputFormat::Json => write!(f, "json"),
        }
    }
}

impl From<CliOutputFormat> for OutputFormat {
    fn from(format: CliOutputFormat) -> Self {
        match format {
            CliOutputFormat::Aider => OutputFormat::Aider,
            CliOutputFormat::Json => OutputFormat::Json,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliEncoding {
    #[value(name = "cl100k_base")]
    Cl100kBase,
    #[value(name = "o200k_base")]
    O200kBase,
}

impl CliEncoding {
    fn as_str(&self) -> &'static str {
        match self {
            CliEncoding::Cl100kBase => "cl100k_base",
            CliEncoding::O200kBase => "o200k_base",
        }
    }
}

impl std::fmt::Display for CliEncoding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "repomap",
    about = "Repository map generator for LLM code assistants"
)]
struct Cli {
    /// Target directory
    #[arg(default_value = ".")]
    root: PathBuf,

    /// One or more paths to bias ranking
    #[arg(long, value_name = "FILE", num_args = 1..)]
    focus: Vec<PathBuf>,

    /// Maximum token budget
    #[arg(long, value_name = "N", default_value_t = 1024)]
    max_tokens: usize,

    /// Output format
    #[arg(long, value_enum, default_value_t = CliOutputFormat::Aider)]
    format: CliOutputFormat,

    /// Tokenizer encoding
    #[arg(long, value_enum, default_value_t = CliEncoding::Cl100kBase)]
    encoding: CliEncoding,
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    // 1. Canonicalize or resolve root directory
    let root = cli.root.canonicalize()?;

    // 2. Scan repository
    let files = scanner::scan_repository(&root)?;

    // 3. Parse files
    let mut file_tags = Vec::new();
    for file in files {
        let full_path = root.join(&file);
        let content = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Some(ft) = parser::parse_file(&file, &content)? {
            file_tags.push(ft);
        }
    }

    // 4. Construct graph
    let graph = RepoGraph::from_file_tags(&file_tags);

    // 5. Compute PageRank
    let ranked = graph.compute_pagerank(&cli.focus);

    // 6. Fit to budget
    let plan = budget::fit_to_budget(&ranked, cli.max_tokens, cli.encoding.as_str())?;

    // 7. Format repomap
    let output = formatter::format_repomap(&plan, cli.format.into())?;

    // 8. Print output to stdout
    if output.ends_with('\n') {
        print!("{output}");
    } else if !output.is_empty() {
        println!("{output}");
    }

    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
