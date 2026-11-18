use super::*;
use crate::parser::Parser;

const SRC: &str = "library L {\n\
                   integer G = 10;\n\
                   function Add(integer a, integer b) -> integer {\n    \
                       return a + b;\n\
                   }\n\
                   function T() {\n    \
                       Run(function() { x = 1; });\n\
                   }\n\
                   }";

fn program() -> Program {
    Parser::parse_str(SRC).unwrap()
}

#[test]
fn finds_the_deepest_expr_at_an_exact_position() {
    let p = program();
    // Column 12 is the 'a' in `return a + b`.
    match find_node_at(&p, 4, 12).unwrap() {
        AstRef::Expr(e) => assert_eq!(e.kind, ExprKind::Var("a".into())),
        other => panic!("expected an Expr, got {other:?}"),
    }
}

#[test]
fn finds_the_enclosing_binary_for_a_range_spanning_both_operands() {
    let p = program();
    match find_node_in(&p, 4, 12, 17).unwrap() {
        AstRef::Expr(e) => assert!(matches!(e.kind, ExprKind::Binary(..))),
        other => panic!("expected an Expr, got {other:?}"),
    }
}

#[test]
fn falls_back_to_statements_and_declarations() {
    let p = program();
    assert!(matches!(
        find_node_at(&p, 4, 1).unwrap(),
        AstRef::Stmt(Stmt::Return { .. })
    ));
    assert!(matches!(
        find_node_at(&p, 2, 1).unwrap(),
        AstRef::Member(Member::Var(_))
    ));
    assert!(matches!(
        find_node_at(&p, 1, 3).unwrap(),
        AstRef::Library(_)
    ));
}

#[test]
fn looks_inside_anonymous_functions() {
    let p = program();
    // Column 26 is the `1` in `x = 1;` inside the anonymous function.
    match find_node_at(&p, 7, 26).unwrap() {
        AstRef::Expr(e) => assert_eq!(e.kind, ExprKind::IntLiteral(1)),
        other => panic!("expected an Expr, got {other:?}"),
    }
}

#[test]
fn returns_none_outside_the_program() {
    assert!(find_node_at(&program(), 999, 1).is_none());
}
