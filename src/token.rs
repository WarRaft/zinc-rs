//! Zinc's token vocabulary: which of the shared [`Kw`]s are active, on top
//! of the shared [`parser_rs::TokenKind`] (identifiers, literals, [`Punct`]).

pub use parser_rs::{Kw, KwSet, Punct};

/// Every Zinc keyword.
pub const ZINC_KEYWORDS: KwSet = KwSet::of(&[
    Kw::Library,
    Kw::Requires,
    Kw::Needs,
    Kw::Uses,
    Kw::Optional,
    Kw::Private,
    Kw::Public,
    Kw::Static,
    Kw::Constant,
    Kw::Readonly,
    Kw::Delegate,
    Kw::Stub,
    Kw::Function,
    Kw::Method,
    Kw::Operator,
    Kw::Struct,
    Kw::Interface,
    Kw::Module,
    Kw::Extends,
    Kw::Type,
    Kw::If,
    Kw::Else,
    Kw::While,
    Kw::Do,
    Kw::For,
    Kw::Break,
    Kw::Return,
    Kw::Debug,
    Kw::True,
    Kw::False,
    Kw::Null,
    Kw::This,
    Kw::ThisType,
]);

/// A Zinc token kind (the shared one).
pub type TokenKind = parser_rs::TokenKind;

/// A Zinc token.
pub type Token = parser_rs::Token<TokenKind>;
