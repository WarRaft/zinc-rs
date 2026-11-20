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
//!
//! The same pass records every name as a symbol [`Occurrence`]
//! ([`semantic_tokens_and_symbols`]), so the editor's symbol index agrees
//! with the highlighting:
//!
//! - **Library members** (functions, variables, structs, interfaces,
//!   modules, types) are private by default: scoped to their library
//!   ([`SymScope::Local`]). `public`, alone or as a `public { }` group,
//!   makes them project-wide. Outside any library everything is
//!   project-wide, and so are library names.
//! - **Struct members** (fields, methods) are [`SymScope::Member`]: a
//!   name after `.` can't be tied to its type without type inference.
//! - **Parameters and locals** are scoped to their function, anonymous
//!   functions included.
//!
//! Local scopes (libraries and functions) are numbered per file, from
//! `first_scope`, so Zinc embedded in vJASS doesn't collide with the host
//! file's JASS functions.

use std::collections::{HashMap, HashSet};

use crate::lexer::SPEC;
use crate::token::{Kw, Punct, Token, TokenKind};
use parser_rs::semantic::{self, comment_tokens_at, lexical_type};
use parser_rs::{
    Modifiers, Occurrence, Scope as SymScope, SemanticModifier as M, SemanticToken, SemanticType as T,
    SymbolKind as K,
};

/// The built-in primitive types (highlighted as `type.defaultLibrary`).
const PRIMITIVES: &[&str] = &["integer", "real", "boolean", "string", "handle", "code"];

/// Type names that are never symbols: the primitives, and `array` in
/// `struct S extends array`.
fn is_builtin_type(name: &str) -> bool {
    PRIMITIVES.contains(&name) || name == "array"
}

/// Every semantic token in `src`, unencoded.
pub fn semantic_tokens(src: &str) -> Vec<SemanticToken> {
    semantic_tokens_at(src, 1)
}

/// [`semantic_tokens`] for Zinc embedded in a larger file, starting on
/// file line `first_line`: the tokens come out file-absolute, ready to
/// merge with the host file's own.
pub fn semantic_tokens_at(src: &str, first_line: usize) -> Vec<SemanticToken> {
    semantic_tokens_and_symbols_at(src, first_line, 1).0
}

/// Every name declared or referenced in `src`.
pub fn symbols(src: &str) -> Vec<Occurrence> {
    semantic_tokens_and_symbols_at(src, 1, 1).1
}

/// Semantic tokens and symbol occurrences of `src` in one pass, from file
/// line `first_line`, numbering local scopes from `first_scope`. Also
/// returns the next free scope number, for a host file that goes on
/// numbering its own.
pub fn semantic_tokens_and_symbols_at(
    src: &str,
    first_line: usize,
    first_scope: u32,
) -> (Vec<SemanticToken>, Vec<Occurrence>, u32) {
    let lexed = parser_rs::tokenize_lossy_at(src, &SPEC, first_line);
    let mut out = comment_tokens_at(&lexed.comments, src, first_line);
    let mut syms = Vec::new();
    let next = Classifier::new(&lexed.tokens).run(&mut out, &mut syms, first_scope);
    (out, syms, next)
}

fn occurrence(t: &Token, name: &str, kind: K, declaration: bool, scope: SymScope) -> Occurrence {
    Occurrence {
        name: name.to_string(),
        line: t.line,
        col: t.col,
        end_col: t.end_col,
        kind,
        declaration,
        scope,
        namespace: String::new(),
        qualifier: None,
    }
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

struct FunctionScope {
    /// Its local scope number.
    id: u32,
    params: HashSet<String>,
    locals: HashSet<String>,
}

impl FunctionScope {
    fn new(id: u32) -> Self {
        FunctionScope { id, params: HashSet::new(), locals: HashSet::new() }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Vis {
    Private,
    Public,
}

/// What a `{ ... }` is the body of.
enum Body {
    /// A library, with its local scope number.
    Library(u32),
    /// A struct, interface, or module body.
    Type,
    Function(FunctionScope),
    /// Any other block (`if`, `{ }`, ...): transparent for lookups. A
    /// `private { }` / `public { }` group carries its visibility.
    Block(Option<Vis>),
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
    vis: Option<Vis>,
}

/// The innermost library around the current token.
fn library_of(stack: &[(Body, i32)]) -> Option<u32> {
    stack.iter().rev().find_map(|(b, _)| match b {
        Body::Library(id) => Some(*id),
        _ => None,
    })
}

/// Where a library-level declaration is visible: its own `private` /
/// `public`, else its group's, else private (Zinc's default). Outside a
/// library, everywhere.
fn member_scope(stack: &[(Body, i32)], mods: &DeclMods) -> SymScope {
    let Some(lib) = library_of(stack) else {
        return SymScope::Project;
    };
    let group = stack.iter().rev().find_map(|(b, _)| match b {
        Body::Block(Some(v)) => Some(Some(*v)),
        Body::Library(_) => Some(None),
        _ => None,
    });
    match mods.vis.or(group.flatten()).unwrap_or(Vis::Private) {
        Vis::Private => SymScope::Local(lib),
        Vis::Public => SymScope::Project,
    }
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

    fn type_kind(&self, name: &str) -> K {
        if self.symbols.structs.contains(name) {
            K::Struct
        } else if self.symbols.interfaces.contains(name) {
            K::Interface
        } else {
            K::Type
        }
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

    /// Classifies every token into `out` and records every name into
    /// `syms`, numbering local scopes from `first_scope`; returns the next
    /// free number.
    fn run(&self, out: &mut Vec<SemanticToken>, syms: &mut Vec<Occurrence>, first_scope: u32) -> u32 {
        let toks = self.toks;
        let decl = Modifiers::NONE.with(M::Declaration);
        let mut next_scope = first_scope;
        let mut new_scope = || {
            next_scope += 1;
            next_scope - 1
        };
        // Each library's private members, and the project-scope references
        // made inside each library: once the whole file is read, those that
        // name a private member are re-scoped to it (members can be used
        // before they're declared).
        let mut privates: HashMap<u32, HashSet<String>> = HashMap::new();
        let mut lib_refs: Vec<(usize, u32)> = Vec::new();
        // Records a project-wide reference, remembering its library.
        let reference = |syms: &mut Vec<Occurrence>, lib_refs: &mut Vec<(usize, u32)>, lib: Option<u32>, o: Occurrence| {
            if let Some(lib) = lib {
                lib_refs.push((syms.len(), lib));
            }
            syms.push(o);
        };

        // Open bodies, each with the paren depth to restore when it closes.
        let mut stack: Vec<(Body, i32)> = Vec::new();
        // The body the next `{` opens, set by a declaration header.
        let mut pending: Option<Body> = None;
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
                    Body::Library(_) => Some(Context::Library),
                    Body::Type => Some(Context::Type),
                    Body::Function(_) => Some(Context::Function),
                    Body::Block(_) => None,
                })
                .unwrap_or(Context::Library);
            let lib = library_of(&stack);

            let name = match &t.kind {
                TokenKind::Ident(name) => name.as_str(),
                k => {
                    match k {
                        TokenKind::Keyword(Kw::Library) => pending = Some(Body::Library(new_scope())),
                        TokenKind::Keyword(Kw::Requires | Kw::Needs | Kw::Uses) => {
                            in_requires = true;
                        }
                        TokenKind::Keyword(Kw::Struct | Kw::Interface) => {
                            pending = Some(Body::Type);
                        }
                        TokenKind::Keyword(Kw::Module) if is_punct(toks.get(i + 2), Punct::LBrace) => {
                            pending = Some(Body::Type);
                        }
                        // `function name(`, `function(` and `method ...(` open
                        // a signature; `function name` alone is a reference.
                        TokenKind::Keyword(Kw::Function)
                            if is_punct(next, Punct::LParen)
                                || (ident(next).is_some() && is_punct(toks.get(i + 2), Punct::LParen)) =>
                        {
                            pending = Some(Body::Function(FunctionScope::new(new_scope())));
                        }
                        TokenKind::Keyword(Kw::Method) => {
                            pending = Some(Body::Function(FunctionScope::new(new_scope())));
                        }
                        TokenKind::Keyword(Kw::Private) => mods.vis = Some(Vis::Private),
                        TokenKind::Keyword(Kw::Public) => mods.vis = Some(Vis::Public),
                        TokenKind::Keyword(Kw::Static) => mods.is_static = true,
                        TokenKind::Keyword(Kw::Constant) => mods.is_constant = true,
                        TokenKind::Keyword(Kw::Readonly) => mods.is_readonly = true,
                        TokenKind::Punct(Punct::LParen) => {
                            if matches!(pending, Some(Body::Function(_))) && params_depth.is_none() {
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
                            // `private {` / `public {` opens a group.
                            let body = pending.take().unwrap_or(Body::Block(mods.vis));
                            stack.push((body, depth));
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
            // A library-level declaration: its symbol, private members
            // remembered for re-scoping references.
            let mut declare = |kind: K, syms: &mut Vec<Occurrence>| {
                let scope = member_scope(&stack, &mods);
                if let SymScope::Local(id) = scope {
                    privates.entry(id).or_default().insert(name.to_string());
                }
                syms.push(occurrence(t, name, kind, true, scope));
            };
            let type_use = |syms: &mut Vec<Occurrence>, lib_refs: &mut Vec<(usize, u32)>| {
                if !is_builtin_type(name) {
                    let o = occurrence(t, name, self.type_kind(name), false, SymScope::Project);
                    reference(syms, lib_refs, lib, o);
                }
            };
            let token = match kind(prev) {
                Some(TokenKind::Keyword(Kw::Library)) => {
                    syms.push(occurrence(t, name, K::Library, true, SymScope::Project));
                    SemanticToken::at(t, T::Namespace, decl)
                }
                _ if in_requires => {
                    syms.push(occurrence(t, name, K::Library, false, SymScope::Project));
                    SemanticToken::at(t, T::Namespace, Modifiers::NONE)
                }
                Some(TokenKind::Keyword(Kw::Struct)) => {
                    declare(K::Struct, syms);
                    SemanticToken::at(t, T::Struct, decl)
                }
                Some(TokenKind::Keyword(Kw::Interface)) => {
                    declare(K::Interface, syms);
                    SemanticToken::at(t, T::Interface, decl)
                }
                Some(TokenKind::Keyword(Kw::Module)) => {
                    if is_punct(next, Punct::LBrace) {
                        declare(K::Module, syms);
                        SemanticToken::at(t, T::Type, decl)
                    } else {
                        // `module Name;` in a struct: implementing it.
                        let o = occurrence(t, name, K::Module, false, SymScope::Project);
                        reference(syms, &mut lib_refs, lib, o);
                        SemanticToken::at(t, T::Type, Modifiers::NONE)
                    }
                }
                Some(TokenKind::Keyword(Kw::Type)) => {
                    declare(K::Type, syms);
                    SemanticToken::at(t, T::Type, decl)
                }
                Some(TokenKind::Keyword(Kw::Extends)) | Some(TokenKind::Punct(Punct::Arrow)) => {
                    type_use(syms, &mut lib_refs);
                    self.type_ref(t, name)
                }
                Some(TokenKind::Keyword(Kw::Function)) => {
                    if is_punct(next, Punct::LParen) {
                        declare(K::Function, syms);
                        SemanticToken::at(t, T::Function, decl)
                    } else if is_punct(next, Punct::Dot) {
                        type_use(syms, &mut lib_refs);
                        self.type_ref(t, name) // `function Struct.method`
                    } else {
                        let o = occurrence(t, name, K::Function, false, SymScope::Project);
                        reference(syms, &mut lib_refs, lib, o);
                        SemanticToken::at(t, T::Function, Modifiers::NONE)
                    }
                }
                Some(TokenKind::Keyword(Kw::Method | Kw::Operator)) => {
                    syms.push(occurrence(t, name, K::Method, true, SymScope::Member));
                    SemanticToken::at(t, T::Method, decl.with_if(mods.is_static, M::Static))
                }
                Some(TokenKind::Punct(Punct::Dot)) => {
                    let method = is_punct(next, Punct::LParen);
                    let kind = if method { K::Method } else { K::Field };
                    syms.push(occurrence(t, name, kind, false, SymScope::Member));
                    let ty = if method { T::Method } else { T::Property };
                    SemanticToken::at(t, ty, modified)
                }
                // `<type> name`: the type part
                _ if ident(next).is_some() => {
                    type_use(syms, &mut lib_refs);
                    self.type_ref(t, name)
                }
                // `<type> name`, or `, name` continuing a declaration
                _ if is_type_like(prev)
                    || (is_punct(prev, Punct::Comma) && decl_depth == Some(depth)) =>
                {
                    if params_depth.is_some() {
                        if let Some(Body::Function(f)) = pending.as_mut() {
                            f.params.insert(name.to_string());
                            syms.push(occurrence(t, name, K::Parameter, true, SymScope::Local(f.id)));
                        }
                        SemanticToken::at(t, T::Parameter, decl)
                    } else {
                        decl_depth = Some(depth);
                        let readonly = mods.is_constant || mods.is_readonly;
                        match context {
                            Context::Function => {
                                if let Some(f) = innermost_function(&mut stack) {
                                    f.locals.insert(name.to_string());
                                    syms.push(occurrence(t, name, K::Local, true, SymScope::Local(f.id)));
                                }
                                SemanticToken::at(t, T::Variable, decl.with_if(readonly, M::Readonly))
                            }
                            Context::Type => {
                                syms.push(occurrence(t, name, K::Field, true, SymScope::Member));
                                SemanticToken::at(
                                    t,
                                    T::Property,
                                    decl.with_if(readonly, M::Readonly)
                                        .with_if(mods.is_static, M::Static),
                                )
                            }
                            Context::Library => {
                                declare(if mods.is_constant { K::Constant } else { K::Global }, syms);
                                SemanticToken::at(t, T::Variable, decl.with_if(readonly, M::Readonly))
                            }
                        }
                    }
                }
                _ if is_punct(next, Punct::LParen) => {
                    let o = occurrence(t, name, K::Function, false, SymScope::Project);
                    reference(syms, &mut lib_refs, lib, o);
                    SemanticToken::at(t, T::Function, Modifiers::NONE)
                }
                _ => {
                    let f = innermost_function(&mut stack);
                    if let Some(f) = f.as_ref().filter(|f| f.params.contains(name)) {
                        syms.push(occurrence(t, name, K::Parameter, false, SymScope::Local(f.id)));
                        SemanticToken::at(t, T::Parameter, modified)
                    } else if let Some(f) = f.as_ref().filter(|f| f.locals.contains(name)) {
                        syms.push(occurrence(t, name, K::Local, false, SymScope::Local(f.id)));
                        SemanticToken::at(t, T::Variable, modified)
                    } else if self.symbols.structs.contains(name)
                        || self.symbols.interfaces.contains(name)
                        || self.symbols.types.contains(name)
                        || PRIMITIVES.contains(&name)
                    {
                        type_use(syms, &mut lib_refs);
                        self.type_ref(t, name)
                    } else {
                        let readonly = self.symbols.constants.contains(name);
                        let kind = if readonly { K::Constant } else { K::Global };
                        reference(syms, &mut lib_refs, lib, occurrence(t, name, kind, false, SymScope::Project));
                        SemanticToken::at(t, T::Variable, modified.with_if(readonly, M::Readonly))
                    }
                }
            };
            out.push(token);
        }

        // A reference inside a library to one of its private members
        // resolves to that member.
        for (index, lib) in lib_refs {
            if privates.get(&lib).is_some_and(|names| names.contains(&syms[index].name)) {
                syms[index].scope = SymScope::Local(lib);
            }
        }
        next_scope
    }
}

fn innermost_function(stack: &mut [(Body, i32)]) -> Option<&mut FunctionScope> {
    stack.iter_mut().rev().find_map(|(s, _)| match s {
        Body::Function(f) => Some(f),
        _ => None,
    })
}

#[cfg(test)]
#[path = "semantic.test.rs"]
mod tests;
