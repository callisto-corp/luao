use std::path::Path;

use clap::{Parser, Subcommand};
use luao_transpiler::TranspileOptions;

#[derive(Parser)]
#[command(name = "luao", version, about = "Luao language compiler and tools")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Build {
        path: String,
        /// Minify the output (strip whitespace and blank lines)
        #[arg(long)]
        minify: bool,
        /// Mangle property/method/variant names per type
        #[arg(long)]
        mangle: bool,
    },
    Check {
        path: String,
    },
    Lsp,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build {
            path,
            minify,
            mangle,
        } => {
            let options = TranspileOptions { minify, mangle };
            build(&path, &options);
        }
        Commands::Check { path } => check(&path),
        Commands::Lsp => start_lsp().await,
    }
}

fn build(path: &str, options: &TranspileOptions) {
    let input = Path::new(path);

    if input.is_dir() {
        build_directory(input, options);
    } else {
        build_file(input, options);
    }
}

fn build_directory(dir: &Path, options: &TranspileOptions) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("Failed to read directory {}: {}", dir.display(), e);
            std::process::exit(1);
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            build_directory(&path, options);
        } else if path.extension().map_or(false, |ext| ext == "luao") {
            build_file(&path, options);
        }
    }
}

fn build_file(path: &Path, options: &TranspileOptions) {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to read {}: {}", path.display(), e);
            return;
        }
    };

    match luao_transpiler::transpile_with_options(&source, options) {
        Ok(lua_code) => {
            let output_path = path.with_extension("lua");
            match std::fs::write(&output_path, &lua_code) {
                Ok(_) => println!("Built: {} -> {}", path.display(), output_path.display()),
                Err(e) => eprintln!("Failed to write {}: {}", output_path.display(), e),
            }
        }
        Err(errors) => {
            eprintln!("Errors in {}:", path.display());
            for error in &errors {
                eprintln!("  {}", error);
            }
        }
    }
}

fn check(path: &str) {
    let input = Path::new(path);

    if input.is_dir() {
        check_directory(input);
    } else {
        check_file(input);
    }
}

fn check_directory(dir: &Path) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("Failed to read directory {}: {}", dir.display(), e);
            std::process::exit(1);
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            check_directory(&path);
        } else if path.extension().map_or(false, |ext| ext == "luao") {
            check_file(&path);
        }
    }
}

fn check_file(path: &Path) {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to read {}: {}", path.display(), e);
            return;
        }
    };

    let (ast, parse_errors) = luao_parser::parse(&source);

    if !parse_errors.is_empty() {
        eprintln!("Parse errors in {}:", path.display());
        for error in &parse_errors {
            eprintln!("  {}", error);
        }
    }

    let mut resolver = luao_resolver::Resolver::new();
    let symbol_table = resolver.resolve(&ast);
    let checker = luao_checker::Checker::new(&symbol_table);
    let diagnostics = checker.check(&ast);

    if diagnostics.is_empty() && parse_errors.is_empty() {
        println!("{}: OK", path.display());
    } else {
        for diag in &diagnostics {
            eprintln!("  {}: {}", path.display(), diag);
        }
    }
}

async fn start_lsp() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = tower_lsp::LspService::new(|client| {
        luao_lsp::LuaoLanguageServer::new(client)
    });

    tower_lsp::Server::new(stdin, stdout, socket)
        .serve(service)
        .await;
}
