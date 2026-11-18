use super::*;
use crate::token::{Kw, Punct, TokenKind};

fn kinds(src: &str) -> Vec<TokenKind> {
    Lexer::tokenize(src)
        .unwrap()
        .into_iter()
        .map(|t| t.kind)
        .collect()
}

#[test]
fn tokenizes_keywords_and_identifiers() {
    assert_eq!(
        kinds("library Foo requires optional Bar"),
        vec![
            TokenKind::Keyword(Kw::Library),
            TokenKind::Ident("Foo".to_string()),
            TokenKind::Keyword(Kw::Requires),
            TokenKind::Keyword(Kw::Optional),
            TokenKind::Ident("Bar".to_string()),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn tokenizes_literals() {
    assert_eq!(
        kinds("123 4.25 0x1F 'hfoo' \"a\\n\""),
        vec![
            TokenKind::Int(123),
            TokenKind::Real(4.25),
            TokenKind::Int(31),
            TokenKind::Int(0x68666f6f),
            TokenKind::Str("a\n".to_string()),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn tokenizes_c_like_operators() {
    assert_eq!(
        kinds("-> += -= *= /= && || ! != == <= >= % ."),
        vec![
            TokenKind::Punct(Punct::Arrow),
            TokenKind::Punct(Punct::PlusAssign),
            TokenKind::Punct(Punct::MinusAssign),
            TokenKind::Punct(Punct::StarAssign),
            TokenKind::Punct(Punct::SlashAssign),
            TokenKind::Punct(Punct::AndAnd),
            TokenKind::Punct(Punct::OrOr),
            TokenKind::Punct(Punct::Bang),
            TokenKind::Punct(Punct::NotEq),
            TokenKind::Punct(Punct::EqEq),
            TokenKind::Punct(Punct::LtEq),
            TokenKind::Punct(Punct::GtEq),
            TokenKind::Punct(Punct::Percent),
            TokenKind::Punct(Punct::Dot),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn skips_line_block_and_zinc_marker_comments() {
    assert_eq!(
        kinds("//! zinc\n/* multi\nline */ x; // tail\n//! endzinc"),
        vec![
            TokenKind::Ident("x".to_string()),
            TokenKind::Punct(Punct::Semicolon),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn records_token_columns() {
    let toks = Lexer::tokenize("  foo += 1;").unwrap();
    assert_eq!((toks[0].line, toks[0].col, toks[0].end_col), (1, 3, 6));
    assert_eq!((toks[1].col, toks[1].end_col), (7, 9));
}

#[test]
fn reports_lone_ampersand() {
    let err = Lexer::tokenize("a & b").unwrap_err();
    assert!(err.message.contains("unexpected character '&'"));
    assert_eq!(err.col, 3);
}

#[test]
fn reports_unterminated_block_comment() {
    let err = Lexer::tokenize("x /* never").unwrap_err();
    assert!(err.message.contains("unterminated block comment"));
}
