//! Zinc's token vocabulary: its keywords, on top of the shared
//! [`parser_rs::TokenKind`] (identifiers, literals, [`Punct`]uation).

pub use parser_rs::{Keyword, Punct};

parser_rs::keywords! {
    /// Every Zinc keyword.
    pub enum Kw {
        Library = "library",
        Requires = "requires",
        Needs = "needs",
        Uses = "uses",
        Optional = "optional",
        Private = "private",
        Public = "public",
        Static = "static",
        Constant = "constant",
        Readonly = "readonly",
        Delegate = "delegate",
        Stub = "stub",
        Function = "function",
        Method = "method",
        Operator = "operator",
        Struct = "struct",
        Interface = "interface",
        Module = "module",
        Extends = "extends",
        Type = "type",
        If = "if",
        Else = "else",
        While = "while",
        Do = "do",
        For = "for",
        Break = "break",
        Return = "return",
        Debug = "debug",
        True = "true",
        False = "false",
        Null = "null",
        This = "this",
        ThisType = "thistype",
    }
}

/// A Zinc token kind — the shared [`parser_rs::TokenKind`] over [`Kw`].
pub type TokenKind = parser_rs::TokenKind<Kw>;

/// A Zinc token.
pub type Token = parser_rs::Token<TokenKind>;
