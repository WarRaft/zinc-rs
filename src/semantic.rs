//! Semantic highlighting for Zinc: the shared lexical layer (keywords,
//! literals, operators, comments) from [`parser_rs::semantic`], plus
//! identifier classification — libraries, structs, interfaces, modules,
//! types, functions, methods, parameters, fields, locals, globals,
//! constants.
//!
//! Classification works on the (lossy) token stream rather than the AST so
//! that half-typed, unparsable code still highlights. Scopes are tracked
//! by braces: a `{` opened right after a `library`/`struct`/`function`/...
//! header is that construct's body, any other `{` is a plain block.

use std::collections::HashSet;

use crate::lexer::SPEC;
use crate::token::{Kw, Punct, Token, TokenKind};
use parser_rs::semantic::{self, comment_tokens_at, lexical_type};
use parser_rs::{Modifiers, SemanticModifier as M, SemanticToken, SemanticType as T};

/// The built-in primitive types (highlighted as `type.defaultLibrary`).
const PRIMITIVES: &[&str] = &["integer", "real", "boolean", "string", "handle", "code"];

/// Every semantic token in `src`, unencoded.
pub fn semantic_tokens(src: &str) -> Vec<SemanticToken> {
    semantic_tokens_at(src, 1)
}

/// [`semantic_tokens`] for Zinc embedded in a larger file, starting on
/// file line `first_line`: the tokens come out file-absolute, ready to
/// merge with the host file's own.
pub fn semantic_tokens_at(src: &str, first_line: usize) -> Vec<SemanticToken> {
    let lexed = parser_rs::tokenize_lossy_at(src, &SPEC, first_line);
    let mut out = comment_tokens_at(&lexed.comments, src, first_line);
    Classifier::new(&lexed.tokens).run(&mut out);
    out
}

/// [`semantic_tokens`] in the editor's wire format (see
/// [`parser_rs::semantic::encode`]).
pub fn encoded_semantic_tokens(src: &str) -> Vec<u32> {
    semantic::encode(semantic_tokens(src), src)
}

/// Names collected up front, since they can be used before (or far from)
/// their declaration.
#[derive(Default)]
struct Symbols {
    structs: HashSet<String>,
    interfaces: HashSet<String>,
    types: HashSet<String>,
    constants: HashSet<String>,
}

#[derive(Default)]
struct FunctionScope {
    params: HashSet<String>,
    locals: HashSet<String>,
}

/// What a `{ ... }` is the body of.
enum Scope {
    Library,
    /// A struct, interface, or module body.
    Type,
    Function(FunctionScope),
    /// Any other block (`if`, `private { }`, ...): transparent for lookups.
    Block,
}

/// Where a declaration sits, which decides how its name is highlighted.
#[derive(Clone, Copy, PartialEq)]
enum Context {
    Library,
    Type,
    Function,
}

#[derive(Default)]
struct DeclMods {
    is_static: bool,
    is_constant: bool,
    is_readonly: bool,
}

struct Classifier<'a> {
    toks: &'a [Token],
    symbols: Symbols,
}

fn kind(t: Option<&Token>) -> Option<&TokenKind> {
    t.map(|t| &t.kind)
}

fn ident(t: Option<&Token>) -> Option<&str> {
    match kind(t) {
        Some(TokenKind::Ident(name)) => Some(name),
        _ => None,
    }
}

fn is_punct(t: Option<&Token>, p: Punct) -> bool {
    kind(t) == Some(&TokenKind::Punct(p))
}

fn is_assign(t: Option<&Token>) -> bool {
    matches!(
        kind(t),
        Some(TokenKind::Punct(
            Punct::Assign
                | Punct::PlusAssign
                | Punct::MinusAssign
                | Punct::StarAssign
                | Punct::SlashAssign
        ))
    )
}

/// Whether `t` can be the type in a `type name` declaration.
fn is_type_like(t: Option<&Token>) -> bool {
    matches!(
        kind(t),
        Some(TokenKind::Ident(_) | TokenKind::Keyword(Kw::ThisType))
    )
}

impl<'a> Classifier<'a> {
    fn new(toks: &'a [Token]) -> Self {
        let mut symbols = Symbols::default();
        for (i, t) in toks.iter().enumerate() {
            let name = |n: usize| ident(toks.get(i + n)).map(String::from);
            match &t.kind {
                TokenKind::Keyword(Kw::Struct) => symbols.structs.extend(name(1)),
                TokenKind::Keyword(Kw::Interface) => symbols.interfaces.extend(name(1)),
                TokenKind::Keyword(Kw::Module | Kw::Type) => symbols.types.extend(name(1)),
                // `constant [other modifiers] <type> <name>`
                TokenKind::Keyword(Kw::Constant) => {
                    let mut j = i + 1;
                    while matches!(
                        kind(toks.get(j)),
                        Some(TokenKind::Keyword(
                            Kw::Private | Kw::Public | Kw::Static | Kw::Readonly
                        ))
                    ) {
                        j += 1;
                    }
                    if is_type_like(toks.get(j)) {
                        symbols.constants.extend(ident(toks.get(j + 1)).map(String::from));
                    }
                }
                _ => {}
            }
        }
        Classifier { toks, symbols }
    }

    /// A reference to a type name: struct, interface, primitive, or other.
    fn type_ref(&self, t: &Token, name: &str) -> SemanticToken {
        let ty = if self.symbols.structs.contains(name) {
            T::Struct
        } else if self.symbols.interfaces.contains(name) {
            T::Interface
        } else {
            T::Type
        };
        let mods = Modifiers::NONE.with_if(PRIMITIVES.contains(&name), M::DefaultLibrary);
        SemanticToken::at(t, ty, mods)
    }

    fn run(&self, out: &mut Vec<SemanticToken>) {
        let toks = self.toks;
        let decl = Modifiers::NONE.with(M::Declaration);

        // Open bodies, each with the paren depth to restore when it closes.
        let mut stack: Vec<(Scope, i32)> = Vec::new();
        // The body the next `{` opens, set by a declaration header.
        let mut pending: Option<Scope> = None;
        let mut mods = DeclMods::default();
        // Parenthesis/bracket nesting, for telling a declaration's `,`
        // from one inside its initializer.
        let mut depth = 0i32;
        // The depth of a function signature's parameter list being read.
        let mut params_depth: Option<i32> = None;
        // The depth of a statement-level declaration (`integer a, b;`).
        let mut decl_depth: Option<i32> = None;
        let mut in_requires = false;

        for (i, t) in toks.iter().enumerate() {
            let prev = i.checked_sub(1).and_then(|j| toks.get(j));
            let next = toks.get(i + 1);
            let context = stack
                .iter()
                .rev()
                .find_map(|(s, _)| match s {
                    Scope::Library => Some(Context::Library),
                    Scope::Type => Some(Context::Type),
                    Scope::Function(_) => Some(Context::Function),
                    Scope::Block => None,
                })
                .unwrap_or(Context::Library);

            let name = match &t.kind {
                TokenKind::Ident(name) => name.as_str(),
                k => {
                    match k {
                        TokenKind::Keyword(Kw::Library) => pending = Some(Scope::Library),
                        TokenKind::Keyword(Kw::Requires | Kw::Needs | Kw::Uses) => {
                            in_requires = true;
                        }
                        TokenKind::Keyword(Kw::Struct | Kw::Interface) => {
                            pending = Some(Scope::Type);
                        }
                        TokenKind::Keyword(Kw::Module) if is_punct(toks.get(i + 2), Punct::LBrace) => {
                            pending = Some(Scope::Type);
                        }
                        // `function name(`, `function(` and `method ...(` open
                        // a signature; `function name` alone is a reference.
                        TokenKind::Keyword(Kw::Function)
                            if is_punct(next, Punct::LParen)
                                || (ident(next).is_some() && is_punct(toks.get(i + 2), Punct::LParen)) =>
                        {
                            pending = Some(Scope::Function(FunctionScope::default()));
                        }
                        TokenKind::Keyword(Kw::Method) => {
                            pending = Some(Scope::Function(FunctionScope::default()));
                        }
                        TokenKind::Keyword(Kw::Static) => mods.is_static = true,
                        TokenKind::Keyword(Kw::Constant) => mods.is_constant = true,
                        TokenKind::Keyword(Kw::Readonly) => mods.is_readonly = true,
                        TokenKind::Punct(Punct::LParen) => {
                            if matches!(pending, Some(Scope::Function(_))) && params_depth.is_none() {
                                params_depth = Some(depth);
                            }
                            depth += 1;
                        }
                        TokenKind::Punct(Punct::RParen) => {
                            depth -= 1;
                            if params_depth == Some(depth) {
                                params_depth = None;
                            }
                        }
                        TokenKind::Punct(Punct::LBracket) => depth += 1,
                        TokenKind::Punct(Punct::RBracket) => depth -= 1,
                        TokenKind::Punct(Punct::LBrace) => {
                            stack.push((pending.take().unwrap_or(Scope::Block), depth));
                            in_requires = false;
                            mods = DeclMods::default();
                            decl_depth = None;
                            params_depth = None;
                            depth = 0;
                        }
                        TokenKind::Punct(Punct::RBrace) => {
                            depth = stack.pop().map_or(0, |(_, d)| d);
                            mods = DeclMods::default();
                            decl_depth = None;
                        }
                        TokenKind::Punct(Punct::Semicolon) => {
                            pending = None;
                            mods = DeclMods::default();
                            decl_depth = None;
                            params_depth = None;
                            depth = 0;
                        }
                        _ => {}
                    }
                    if let Some(ty) = lexical_type(k) {
                        out.push(SemanticToken::at(t, ty, Modifiers::NONE));
                    }
                    continue;
                }
            };

            let modified = Modifiers::NONE.with_if(is_assign(next), M::Modification);
            let token = match kind(prev) {
                Some(TokenKind::Keyword(Kw::Library)) => SemanticToken::at(t, T::Namespace, decl),
                _ if in_requires => SemanticToken::at(t, T::Namespace, Modifiers::NONE),
                Some(TokenKind::Keyword(Kw::Struct)) => SemanticToken::at(t, T::Struct, decl),
                Some(TokenKind::Keyword(Kw::Interface)) => SemanticToken::at(t, T::Interface, decl),
                Some(TokenKind::Keyword(Kw::Module)) => {
                    let mods = if is_punct(next, Punct::LBrace) { decl } else { Modifiers::NONE };
                    SemanticToken::at(t, T::Type, mods)
                }
                Some(TokenKind::Keyword(Kw::Type)) => SemanticToken::at(t, T::Type, decl),
                Some(TokenKind::Keyword(Kw::Extends)) | Some(TokenKind::Punct(Punct::Arrow)) => {
                    self.type_ref(t, name)
                }
                Some(TokenKind::Keyword(Kw::Function)) => {
                    if is_punct(next, Punct::LParen) {
                        SemanticToken::at(t, T::Function, decl)
                    } else if is_punct(next, Punct::Dot) {
                        self.type_ref(t, name) // `function Struct.method`
                    } else {
                        SemanticToken::at(t, T::Function, Modifiers::NONE)
                    }
                }
                Some(TokenKind::Keyword(Kw::Method | Kw::Operator)) => {
                    SemanticToken::at(t, T::Method, decl.with_if(mods.is_static, M::Static))
                }
                Some(TokenKind::Punct(Punct::Dot)) => {
                    let ty = if is_punct(next, Punct::LParen) { T::Method } else { T::Property };
                    SemanticToken::at(t, ty, modified)
                }
                // `<type> name`: the type part
                _ if ident(next).is_some() => self.type_ref(t, name),
                // `<type> name`, or `, name` continuing a declaration
                _ if is_type_like(prev)
                    || (is_punct(prev, Punct::Comma) && decl_depth == Some(depth)) =>
                {
                    if params_depth.is_some() {
                        if let Some(Scope::Function(f)) = pending.as_mut() {
                            f.params.insert(name.to_string());
                        }
                        SemanticToken::at(t, T::Parameter, decl)
                    } else {
                        decl_depth = Some(depth);
                        let readonly = mods.is_constant || mods.is_readonly;
                        match context {
                            Context::Function => {
                                if let Some(f) = innermost_function(&mut stack) {
                                    f.locals.insert(name.to_string());
                                }
                                SemanticToken::at(t, T::Variable, decl.with_if(readonly, M::Readonly))
                            }
                            Context::Type => SemanticToken::at(
                                t,
                                T::Property,
                                decl.with_if(readonly, M::Readonly)
                                    .with_if(mods.is_static, M::Static),
                            ),
                            Context::Library => {
                                SemanticToken::at(t, T::Variable, decl.with_if(readonly, M::Readonly))
                            }
                        }
                    }
                }
                _ if is_punct(next, Punct::LParen) => {
                    SemanticToken::at(t, T::Function, Modifiers::NONE)
                }
                _ => {
                    let f = innermost_function(&mut stack);
                    if f.as_ref().is_some_and(|f| f.params.contains(name)) {
                        SemanticToken::at(t, T::Parameter, modified)
                    } else if f.as_ref().is_some_and(|f| f.locals.contains(name)) {
                        SemanticToken::at(t, T::Variable, modified)
                    } else if self.symbols.structs.contains(name)
                        || self.symbols.interfaces.contains(name)
                        || self.symbols.types.contains(name)
                        || PRIMITIVES.contains(&name)
                    {
                        self.type_ref(t, name)
                    } else {
                        let readonly = self.symbols.constants.contains(name);
                        SemanticToken::at(t, T::Variable, modified.with_if(readonly, M::Readonly))
                    }
                }
            };
            out.push(token);
        }
    }
}

fn innermost_function(stack: &mut [(Scope, i32)]) -> Option<&mut FunctionScope> {
    stack.iter_mut().rev().find_map(|(s, _)| match s {
        Scope::Function(f) => Some(f),
        _ => None,
    })
}

#[cfg(test)]
#[path = "semantic.test.rs"]
mod tests;
