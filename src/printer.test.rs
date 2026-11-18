use super::*;
use crate::parser::Parser;

#[test]
fn renders_library_tree() {
    let program = Parser::parse_str(
        "library L requires optional T {\n  private constant integer N = 2;\n  function F(integer a) -> integer {\n    return a + N;\n  }\n}",
    )
    .unwrap();
    assert_eq!(
        render_tree(&program),
        "Program\n\
         └─ Library L requires optional T (line 1)\n   \
            ├─ Global private constant integer N (line 2)\n   \
            │  └─ Int 2\n   \
            └─ Function F(integer a) -> integer (line 3)\n      \
               └─ Return (line 4)\n         \
                  └─ Binary Add\n            \
                     ├─ Var a\n            \
                     └─ Var N\n"
    );
}

#[test]
fn renders_struct_and_statement_shapes() {
    let program = Parser::parse_str(
        "library L {\n struct S {\n  static method m() { for (0 <= i < 3) { this.f(i); } }\n }\n}",
    )
    .unwrap();
    let tree = render_tree(&program);
    assert!(tree.contains("Struct S (line 2)"));
    assert!(tree.contains("Method static m() (line 3)"));
    assert!(tree.contains("For i (line 3)"));
    assert!(tree.contains("Start LtEq"));
    assert!(tree.contains("Call this.f"));
}

#[test]
fn renders_dot_graph() {
    let program = Parser::parse_str("library L {}").unwrap();
    let dot = render_dot(&program);
    assert!(dot.starts_with("digraph AST {"));
    assert!(dot.contains("n0 -> n1;"));
}
