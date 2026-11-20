//! A parser and linter for Zinc — the C-like Warcraft III scripting dialect
//! embedded in vJASS between `//! zinc` and `//! endzinc`. Structured
//! exactly like `jass-rs`; everything not Zinc-specific (spans, lexer
//! cursor, literal scanners, token stream, diagnostics, tree renderers, the
//! HTML viewer) comes from the shared `parser-rs` crate.

pub mod ast;
pub mod html;
pub mod lexer;
pub mod lint;
pub mod parser;
pub mod printer;
pub mod query;
pub mod semantic;
pub mod token;

pub use ast::{ColumnEncoding, Program, Span};
pub use html::render_html;
pub use lint::{lint, Diagnostic, Level};
pub use parser::{ParseError, Parser};
pub use printer::{render_dot, render_tree};
pub use query::{find_node_at, find_node_in, AstRef};
pub use semantic::{
    encoded_semantic_tokens, semantic_tokens, semantic_tokens_and_symbols_at, semantic_tokens_at, symbols,
};

/// Parses Zinc source into an AST.
pub fn parse(src: &str) -> Result<Program, ParseError> {
    Parser::parse_str(src)
}

/// Parses Zinc embedded in a larger file, starting on file line
/// `first_line`: the entry point a host parser calls for a `//! zinc`
/// block ([`parser_rs::Embedded`]). Positions stay file-absolute.
pub fn parse_at(src: &str, first_line: usize) -> Result<Program, ParseError> {
    Parser::parse_str_at(src, first_line)
}
