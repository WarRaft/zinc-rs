use std::env;
use std::fs;
use std::process::ExitCode;

use zinc_rs::{lint, render_dot, render_html, render_tree, Level};

enum Mode {
    Lint,
    Ast,
    Dot,
    Html,
}

fn usage() {
    eprintln!("usage: zinc-rs [--ast|--dot|--html] <file.zn>");
    eprintln!("  (no flag)  lint the file and print diagnostics");
    eprintln!("  --ast      print the parsed AST as an indented tree");
    eprintln!("  --dot      print the parsed AST as a Graphviz DOT graph");
    eprintln!("  --html     print a self-contained AST viewer page (source + tree)");
    eprintln!();
    eprintln!("  e.g. zinc-rs --html file.zn > file.ast.html && open file.ast.html");
}

fn main() -> ExitCode {
    let mut mode = Mode::Lint;
    let mut path = None;
    for arg in env::args().skip(1) {
        match arg.as_str() {
            "--ast" => mode = Mode::Ast,
            "--dot" => mode = Mode::Dot,
            "--html" => mode = Mode::Html,
            "-h" | "--help" => {
                usage();
                return ExitCode::SUCCESS;
            }
            other => path = Some(other.to_string()),
        }
    }

    let Some(path) = path else {
        usage();
        return ExitCode::FAILURE;
    };

    let src = match fs::read_to_string(&path) {
        Ok(src) => src,
        Err(err) => {
            eprintln!("{path}: {err}");
            return ExitCode::FAILURE;
        }
    };

    let program = match zinc_rs::parse(&src) {
        Ok(program) => program,
        Err(err) => {
            eprintln!("{path}:{}:{}: error: {}", err.line, err.col, err.message);
            return ExitCode::FAILURE;
        }
    };

    match mode {
        Mode::Ast => print!("{}", render_tree(&program)),
        Mode::Dot => print!("{}", render_dot(&program)),
        Mode::Html => print!("{}", render_html(&program, &src, &path)),
        Mode::Lint => {
            let diagnostics = lint(&program);
            for diag in &diagnostics {
                println!(
                    "{path}:{}: {}: {}",
                    diag.line,
                    diag.level.as_str(),
                    diag.message
                );
            }
            if diagnostics.is_empty() {
                println!("{path}: no issues found");
            }
            if diagnostics.iter().any(|d| d.level == Level::Error) {
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}
