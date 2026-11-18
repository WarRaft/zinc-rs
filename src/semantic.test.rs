use super::*;
use parser_rs::SemanticModifier as M;

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
