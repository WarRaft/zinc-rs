use super::*;
use parser_rs::SemanticModifier as M;
use parser_rs::{Scope as S, SymbolKind as K};

const SRC: &str = include_str!("../tests/fixtures/sample.zn");

/// The token covering the `nth` (0-based) whole-word occurrence of `word`
/// on `line`.
fn find(line: usize, word: &str, nth: usize) -> (T, Modifiers) {
    let toks = semantic_tokens(SRC);
    let text = SRC.lines().nth(line - 1).unwrap();
    let col = text
        .match_indices(word)
        .filter(|(i, _)| {
            let word_char = |c: char| c.is_alphanumeric() || c == '_';
            !text[..*i].chars().last().is_some_and(word_char)
                && !text[i + word.len()..].chars().next().is_some_and(word_char)
        })
        .nth(nth)
        .map(|(i, _)| text[..i].chars().count() + 1)
        .unwrap_or_else(|| panic!("{word:?} not on line {line}: {text:?}"));
    let t = toks
        .iter()
        .find(|t| t.line == line && t.start_col == col)
        .unwrap_or_else(|| panic!("no token for {word:?} at {line}:{col}"));
    (t.ty, t.mods)
}

fn m(mods: &[M]) -> Modifiers {
    Modifiers::of(mods)
}

#[test]
fn classifies_library_level_declarations() {
    assert_eq!(find(1, "//!", 0).0, T::Comment);
    // inside the multi-line block comment
    assert!(semantic_tokens(SRC).iter().any(|t| t.line == 3 && t.ty == T::Comment));
    assert_eq!(find(7, "Counters", 0), (T::Namespace, m(&[M::Declaration])));
    assert_eq!(find(7, "TimerUtils", 0), (T::Namespace, m(&[])));
    assert_eq!(find(8, "MAX_COUNTERS", 0), (T::Variable, m(&[M::Declaration, M::Readonly])));
    assert_eq!(find(8, "integer", 0), (T::Type, m(&[M::DefaultLibrary])));
    assert_eq!(find(9, "counters", 0), (T::Variable, m(&[M::Declaration])));
    assert_eq!(find(9, "MAX_COUNTERS", 0), (T::Variable, m(&[M::Readonly])));
    assert_eq!(find(10, "ratio", 0), (T::Variable, m(&[M::Declaration])));
    assert_eq!(find(10, "scale", 0), (T::Variable, m(&[M::Declaration])));
    assert_eq!(find(13, "NAME", 0), (T::Variable, m(&[M::Declaration])));
    assert_eq!(find(36, "IsEven", 0), (T::Function, m(&[M::Declaration])));
    assert_eq!(find(36, "n", 0), (T::Parameter, m(&[M::Declaration])));
    assert_eq!(find(36, "boolean", 0), (T::Type, m(&[M::DefaultLibrary])));
}

#[test]
fn classifies_struct_members() {
    assert_eq!(find(16, "Counter", 0), (T::Struct, m(&[M::Declaration])));
    assert_eq!(find(17, "value", 0), (T::Property, m(&[M::Declaration])));
    assert_eq!(find(18, "created", 0), (T::Property, m(&[M::Declaration, M::Static])));
    assert_eq!(find(20, "create", 0), (T::Method, m(&[M::Declaration, M::Static])));
    assert_eq!(find(20, "start", 0), (T::Parameter, m(&[M::Declaration])));
    assert_eq!(find(21, "c", 0), (T::Variable, m(&[M::Declaration])));
    assert_eq!(find(21, "allocate", 0), (T::Method, m(&[])));
    assert_eq!(find(22, "c", 0), (T::Variable, m(&[])));
    assert_eq!(find(22, "value", 0), (T::Property, m(&[M::Modification])));
    assert_eq!(find(22, "start", 0), (T::Parameter, m(&[])));
    assert_eq!(find(27, "i", 0), (T::Parameter, m(&[M::Declaration])));
    assert_eq!(find(31, "bump", 0), (T::Method, m(&[M::Declaration])));
}

#[test]
fn classifies_function_bodies() {
    assert_eq!(find(41, "i", 0), (T::Variable, m(&[M::Declaration])));
    assert_eq!(find(41, "total", 0), (T::Variable, m(&[M::Declaration])));
    assert_eq!(find(44, "total", 0), (T::Variable, m(&[M::Modification])));
    assert_eq!(find(43, "IsEven", 0), (T::Function, m(&[])));
    assert_eq!(find(54, "Counter", 0), (T::Struct, m(&[])));
    assert_eq!(find(54, "c", 0), (T::Variable, m(&[M::Declaration])));
    assert_eq!(find(54, "Counter", 1), (T::Struct, m(&[])));
    assert_eq!(find(54, "create", 0), (T::Method, m(&[])));
    assert_eq!(find(56, "counters", 0), (T::Variable, m(&[])));
    assert_eq!(find(57, "i", 0), (T::Variable, m(&[M::Modification])));
    assert_eq!(find(62, "debug", 0), (T::Keyword, m(&[])));
    assert_eq!(find(62, "NAME", 0), (T::Variable, m(&[])));
    assert_eq!(find(62, "SumCounters", 0), (T::Function, m(&[])));
    assert_eq!(find(63, "c", 0), (T::Variable, m(&[])));
    assert_eq!(find(63, "bump", 0), (T::Method, m(&[])));
    assert_eq!(find(64, "ratio", 0), (T::Variable, m(&[])));
    assert_eq!(find(65, "DestroyTimer", 0), (T::Function, m(&[])));
}

#[test]
fn anonymous_functions_keep_the_surrounding_paren_depth() {
    // After `function() { ... }` closes inside `TimerStart(`, the `;` and
    // the next declaration must still classify normally.
    let src = "library L {\n function F() {\n  G(function() { integer a; });\n  integer b, c;\n }\n}";
    let toks = semantic_tokens(src);
    let at = |line, col| toks.iter().find(|t| t.line == line && t.start_col == col).map(|t| (t.ty, t.mods));
    let decl = Some((T::Variable, m(&[M::Declaration])));
    assert_eq!(at(3, 26), decl); // a
    assert_eq!(at(4, 11), decl); // b
    assert_eq!(at(4, 14), decl); // c
}

#[test]
fn still_highlights_unparsable_code() {
    let src = "library L {\n function F( {\n  x = \"open\n  G();\n";
    assert!(crate::parse(src).is_err());
    let toks = semantic_tokens(src);
    assert!(toks.iter().any(|t| t.line == 4 && t.ty == T::Function));
}

#[test]
fn highlights_the_global_scope_and_stray_statements() {
    let src = "constant integer LIMIT = 5;\nunit hero;\nfunction Tick() { hero = null; }\nif (LIMIT > 0) { Tick(); }";
    let toks = semantic_tokens(src);
    let at = |line, col| toks.iter().find(|t| t.line == line && t.start_col == col).map(|t| (t.ty, t.mods));
    let m = Modifiers::of;
    assert_eq!(at(1, 18), Some((T::Variable, m(&[M::Declaration, M::Readonly]))));
    assert_eq!(at(2, 6), Some((T::Variable, m(&[M::Declaration]))));
    assert_eq!(at(3, 10), Some((T::Function, m(&[M::Declaration]))));
    assert_eq!(at(3, 19), Some((T::Variable, m(&[M::Modification]))));
    assert_eq!(at(4, 1), Some((T::Keyword, m(&[]))));
    assert_eq!(at(4, 5), Some((T::Variable, m(&[M::Readonly]))));
    assert_eq!(at(4, 18), Some((T::Function, m(&[]))));
}

/// Every occurrence of `name` in `src` as (line, kind, declaration, scope).
fn occ(src: &str, name: &str) -> Vec<(usize, K, bool, S)> {
    symbols(src)
        .into_iter()
        .filter(|o| o.name == name)
        .map(|o| (o.line, o.kind, o.declaration, o.scope))
        .collect()
}

// In sample.zn the library `Counters` is local scope 1; then, in order,
// the functions: create 2, operator [] 3, bump 4, IsEven 5, SumCounters 6,
// onInit 7, the anonymous timer callback 8.
const LIB: S = S::Local(1);

#[test]
fn libraries_are_project_wide() {
    assert_eq!(occ(SRC, "Counters"), vec![(7, K::Library, true, S::Project)]);
    assert_eq!(occ(SRC, "TimerUtils"), vec![(7, K::Library, false, S::Project)]);
}

#[test]
fn library_members_are_private_by_default() {
    assert_eq!(
        occ(SRC, "MAX_COUNTERS"),
        vec![(8, K::Constant, true, LIB), (9, K::Constant, false, LIB), (42, K::Constant, false, LIB), (55, K::Constant, false, LIB)]
    );
    // Declared in a `private { }` group, used further down.
    assert_eq!(occ(SRC, "NAME"), vec![(13, K::Global, true, LIB), (62, K::Global, false, LIB)]);
    // Used before... and after its declaration, all in the library.
    assert_eq!(occ(SRC, "IsEven"), vec![(36, K::Function, true, LIB), (43, K::Function, false, LIB)]);
}

#[test]
fn public_members_and_outside_names_are_project_wide() {
    // `public struct Counter`, used as a type and for a static call.
    assert_eq!(
        occ(SRC, "Counter"),
        vec![(16, K::Struct, true, S::Project), (54, K::Struct, false, S::Project), (54, K::Struct, false, S::Project)]
    );
    // Not declared here: a project-wide reference.
    assert_eq!(occ(SRC, "BJDebugMsg"), vec![(62, K::Function, false, S::Project)]);
}

#[test]
fn struct_members_are_members() {
    assert_eq!(
        occ(SRC, "value"),
        vec![(17, K::Field, true, S::Member), (22, K::Field, false, S::Member), (32, K::Field, false, S::Member)]
    );
    assert_eq!(occ(SRC, "create"), vec![(20, K::Method, true, S::Member), (54, K::Method, false, S::Member)]);
    assert_eq!(occ(SRC, "bump"), vec![(31, K::Method, true, S::Member), (63, K::Method, false, S::Member)]);
}

#[test]
fn parameters_and_locals_belong_to_their_function() {
    assert_eq!(occ(SRC, "start"), vec![(20, K::Parameter, true, S::Local(2)), (22, K::Parameter, false, S::Local(2))]);
    let i: Vec<_> = occ(SRC, "i").into_iter().map(|(line, _, decl, scope)| (line, decl, scope)).collect();
    assert_eq!(i[0], (27, true, S::Local(3))); // operator []'s parameter
    assert_eq!(i[2], (41, true, S::Local(6))); // SumCounters' local
    assert!(i.contains(&(53, true, S::Local(7)))); // onInit's own `i`
    assert!(i.iter().all(|(_, _, scope)| matches!(scope, S::Local(3 | 6 | 7))));
}

#[test]
fn primitives_and_array_are_not_symbols() {
    assert!(occ(SRC, "integer").is_empty());
    assert!(occ("struct S extends array { }", "array").is_empty());
}

#[test]
fn visibility_follows_modifiers_and_groups() {
    let src = "\
library A {
    public function Api() { Hidden(); }
    function Hidden() { }
    public { integer shared; }
    private { integer secret; }
}
function G() { Api(); Hidden(); shared = 1; }
";
    let p = S::Project;
    assert_eq!(occ(src, "Api"), vec![(2, K::Function, true, p), (7, K::Function, false, p)]);
    // Private: the library's own use is its member; G's is something else.
    assert_eq!(occ(src, "Hidden"), vec![(2, K::Function, false, S::Local(1)), (3, K::Function, true, S::Local(1)), (7, K::Function, false, p)]);
    assert_eq!(occ(src, "shared"), vec![(4, K::Global, true, p), (7, K::Global, false, p)]);
    assert_eq!(occ(src, "secret"), vec![(5, K::Global, true, S::Local(1))]);
}

#[test]
fn two_libraries_privates_are_different_symbols() {
    let src = "library A { integer n; function F() { n = 1; } }\nlibrary B { integer n; function F() { n = 2; } }\n";
    let n = occ(src, "n");
    assert_eq!(n.iter().filter(|o| o.3 == S::Local(1)).count(), 2);
    assert_eq!(n.iter().filter(|o| o.3 == S::Local(3)).count(), 2);
}

#[test]
fn embedded_blocks_number_scopes_from_the_hosts_counter() {
    let (_, syms, next) = semantic_tokens_and_symbols_at("library L { function F(integer x) { } }", 10, 5);
    let x = syms.iter().find(|o| o.name == "x").unwrap();
    assert_eq!((x.line, x.scope), (10, S::Local(6)));
    assert_eq!(next, 7);
}
