use crate::ast::Program;
use crate::printer::build_tree;
use crate::token::{Keyword, Kw};
use parser_rs::HtmlLang;

/// Zinc's flavor of the shared viewer: its keyword set, plus `/* */`.
pub const HTML_LANG: HtmlLang = HtmlLang {
    is_keyword: |word| Kw::from_word(word).is_some(),
    block_comments: true,
};

/// Renders a self-contained HTML page that visualizes a parsed program's
/// AST next to its highlighted source — see [`parser_rs::html`].
pub fn render_html(program: &Program, source: &str, title: &str) -> String {
    parser_rs::render_html(&build_tree(program), source, title, &HTML_LANG)
}

#[cfg(test)]
#[path = "html.test.rs"]
mod tests;
