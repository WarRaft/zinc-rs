use crate::token::{ZINC_KEYWORDS, Token};
use parser_rs::{LexSpec, ParseError, Punct};

/// Zinc's lexical rules for the shared [`parser_rs::lexer`]: `//` and
/// `/* */` comments (so `//! zinc` markers are just comments), rawcodes,
/// and a C-like operator set.
pub const SPEC: LexSpec = LexSpec {
    keywords: ZINC_KEYWORDS,
    line_comment: "//",
    block_comment: Some(("/*", "*/")),
    rawcodes: true,
    embeds: &[],
    puncts: &[
        Punct::Plus,
        Punct::Minus,
        Punct::Star,
        Punct::Slash,
        Punct::Percent,
        Punct::Bang,
        Punct::AndAnd,
        Punct::OrOr,
        Punct::EqEq,
        Punct::NotEq,
        Punct::Gt,
        Punct::Lt,
        Punct::GtEq,
        Punct::LtEq,
        Punct::Assign,
        Punct::PlusAssign,
        Punct::MinusAssign,
        Punct::StarAssign,
        Punct::SlashAssign,
        Punct::Arrow,
        Punct::LParen,
        Punct::RParen,
        Punct::LBracket,
        Punct::RBracket,
        Punct::LBrace,
        Punct::RBrace,
        Punct::Comma,
        Punct::Semicolon,
        Punct::Dot,
    ],
};

pub struct Lexer;

impl Lexer {
    pub fn tokenize(src: &str) -> Result<Vec<Token>, ParseError> {
        Lexer::tokenize_at(src, 1)
    }

    /// Lexes a region of a larger file that starts on `first_line` (e.g. a
    /// vJASS `//! zinc` block), keeping file-absolute positions.
    pub fn tokenize_at(src: &str, first_line: usize) -> Result<Vec<Token>, ParseError> {
        parser_rs::tokenize_at(src, &SPEC, first_line)
    }
}

#[cfg(test)]
#[path = "lexer.test.rs"]
mod tests;
