use super::*;
use crate::parser::Parser;

fn lint_src(src: &str) -> Vec<Diagnostic> {
    lint(&Parser::parse_str(src).expect("source should parse"))
}

fn lint_body(body: &str) -> Vec<Diagnostic> {
    lint_src(&format!("library L {{ function F() {{ {body} }} }}"))
}

fn has(diags: &[Diagnostic], level: Level, text: &str) -> bool {
    diags
        .iter()
        .any(|d| d.level == level && d.message.contains(text))
}

#[test]
fn flags_duplicate_libraries_members_and_struct_members() {
    let d = lint_src("library A {}\nlibrary A {}");
    assert!(has(&d, Level::Error, "duplicate library 'A'"));

    let d = lint_src("library L { integer x; function x() {} }");
    assert!(has(&d, Level::Error, "duplicate library member 'x'"));

    let d = lint_src("library L { struct S { integer v; method v() {} } }");
    assert!(has(&d, Level::Error, "duplicate struct member 'v'"));
}

#[test]
fn flags_duplicate_and_unused_locals() {
    let d = lint_body("integer x = 1; integer x = 2; F2(x);");
    assert!(has(&d, Level::Error, "duplicate local variable 'x'"));

    let d = lint_src("library L { function F(integer a) { integer a; } }");
    assert!(has(&d, Level::Error, "duplicate local variable 'a'"));

    let d = lint_body("integer unused = 5;");
    assert!(has(&d, Level::Warning, "unused local variable 'unused'"));
}

#[test]
fn plain_assignment_is_not_a_use_but_compound_and_for_are() {
    assert!(has(&lint_body("integer x; x = 1;"), Level::Warning, "unused"));
    assert!(lint_body("integer x; x += 1;").is_empty());
    assert!(lint_body("integer i; for (0 <= i < 3) {}").is_empty());
}

#[test]
fn flags_infinite_loops_without_exit() {
    let d = lint_body("while (true) { F2(); }");
    assert!(has(&d, Level::Warning, "always true"));

    // A `break` in a nested loop doesn't leave the outer one...
    let d = lint_body("while (true) { while (true) { break; } }");
    assert_eq!(d.len(), 1);

    // ...but one in an `if` does, and so does `return`.
    assert!(lint_body("while (true) { if (G()) { break; } }").is_empty());
    assert!(lint_body("do { return; } while (true);").is_empty());
}

#[test]
fn flags_break_outside_loop() {
    assert!(has(&lint_body("break;"), Level::Error, "'break' outside"));
    assert!(lint_body("while (G()) { if (H()) { break; } }").is_empty());
}

#[test]
fn flags_empty_if_and_else_bodies() {
    let d = lint_body("if (G()) {} else {}");
    assert!(has(&d, Level::Warning, "empty 'if' body"));
    assert!(has(&d, Level::Warning, "empty 'else' body"));
}

#[test]
fn flags_missing_return_including_in_anonymous_functions() {
    let d = lint_src("library L { function F() -> integer {} }");
    assert!(has(&d, Level::Warning, "function 'F' declares a return type"));

    let d = lint_body("G(function() -> integer { integer u; });");
    assert!(has(&d, Level::Warning, "anonymous function declares a return type"));
    assert!(has(&d, Level::Warning, "unused local variable 'u'"));
}

#[test]
fn interface_signatures_are_not_checked_for_returns() {
    assert!(lint_src("library L { interface I { method get() -> integer; } }").is_empty());
}

#[test]
fn diagnostics_are_sorted_by_line() {
    let d = lint_src("library L {\nfunction F() -> integer {\ninteger u;\n}\n}");
    let lines: Vec<usize> = d.iter().map(|d| d.line).collect();
    assert_eq!(lines, vec![2, 3]);
}

#[test]
fn global_scope_is_linted_like_a_library_body() {
    let d = lint_src("integer x;\nreal x;\nfunction F() { integer unused; }\nprivate integer y;");
    assert!(has(&d, Level::Error, "duplicate global 'x' (first declared at line 1)"), "{d:?}");
    assert!(has(&d, Level::Warning, "unused local variable 'unused'"), "{d:?}");
    assert!(has(&d, Level::Warning, "'private' has no effect outside a library"), "{d:?}");
}

#[test]
fn asks_to_move_statements_outside_functions_into_one() {
    let d = lint_src("if (true) { f(); }\nx = 1;\nf();\nwhile (true) { break; }");
    let errors: Vec<(usize, &str)> = d
        .iter()
        .filter(|d| d.level == Level::Error)
        .map(|d| (d.line, d.message.as_str()))
        .collect();
    assert_eq!(
        errors,
        [
            (1, "'if' statement outside a function: move it into a function"),
            (2, "assignment outside a function: move it into a function"),
            (3, "call outside a function: move it into a function"),
            (4, "'while' loop outside a function: move it into a function"),
        ]
    );
}

#[test]
fn an_empty_else_is_marked_from_else_and_its_fix_deletes_it() {
    let src = "function F() {\n    if (G()) { H(); }\n    else {\n    }\n}";
    let d = lint_src(src).into_iter().find(|d| d.message == "empty 'else' body").expect("reported");
    assert_eq!(d.span, Some(Span::new(3, 5, 4, 6)));
    let edit = &d.fix.expect("a fix").edits[0];
    // From just after the `if` body's `}` to the `else` body's `}`.
    assert_eq!(edit.span, Span::new(2, 22, 4, 6));
    assert_eq!(edit.text, "");
}
