use super::*;
use crate::parser::Parser;

#[test]
fn renders_zinc_source_and_tree() {
    let src = "/* header\n */\nlibrary L { function F() { x = 1; } }";
    let program = Parser::parse_str(src).unwrap();
    let html = render_html(&program, src, "demo.zn");
    assert!(html.contains("demo.zn"));
    assert!(html.contains("\"label\":\"Program\""));
    assert!(html.contains(r#"<span class="tok tok-kw" data-scol="1" data-ecol="8">library</span>"#));
    // The block comment's second line is still highlighted as a comment.
    assert!(html.contains(r#"<span class="tok tok-com" data-scol="1" data-ecol="4"> */</span>"#));
}

#[test]
fn expression_node_spans_align_with_source_token_columns() {
    let src = "library L { function F() { x = 1; } }";
    let program = Parser::parse_str(src).unwrap();
    let html = render_html(&program, src, "demo.zn");
    assert!(html.contains("\"label\":\"Int 1\",\"kind\":\"lit\",\"line\":1,\"eline\":1,\"scol\":32,\"ecol\":33"));
    assert!(html.contains(r#"<span class="tok tok-num" data-scol="32" data-ecol="33">1</span>"#));
}
