use crate::token::{Kw, Token};
use parser_rs::{LexSpec, ParseError, Punct};

/// Zinc's lexical rules for the shared [`parser_rs::lexer`]: `//` and
/// `/* */` comments (so `//! zinc` markers are just comments), rawcodes,
/// and a C-like operator set.
pub const SPEC: LexSpec = LexSpec {
    line_comment: "//",
    block_comment: Some(("/*", "*/")),
    rawcodes: true,
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
        parser_rs::tokenize::<Kw>(src, &SPEC)
    }
}

#[cfg(test)]
#[path = "lexer.test.rs"]
mod tests;
