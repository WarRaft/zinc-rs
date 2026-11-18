//! Position-based lookup over a parsed [`Program`] — "which AST node is at
//! (or covers) this location".
//!
//! Coordinates are in [`Span`]'s native representation (line,
//! codepoint-counted column); see [`ColumnEncoding`] for converting from
//! and to other encodings.

use crate::ast::*;

/// A reference to whichever kind of AST node a query matched.
#[derive(Debug, Clone, Copy)]
pub enum AstRef<'a> {
    Library(&'a Library),
    Member(&'a Member),
    StructMember(&'a StructMember),
    Stmt(&'a Stmt),
    Expr(&'a Expr),
}

impl AstRef<'_> {
    /// The node's location. Declarations and statements only carry a
    /// starting line (see [`Span::whole_line`]); expressions carry a
    /// precise column range.
    pub fn span(&self) -> Span {
        match self {
            AstRef::Library(l) => Span::whole_line(l.line),
            AstRef::Member(m) => Span::whole_line(member_line(m)),
            AstRef::StructMember(m) => Span::whole_line(struct_member_line(m)),
            AstRef::Stmt(s) => Span::whole_line(s.line()),
            AstRef::Expr(e) => e.span,
        }
    }
}

fn member_line(m: &Member) -> usize {
    match m {
        Member::Var(d) => d.line,
        Member::Function(f) => f.sig.line,
        Member::Struct(s) => s.line,
        Member::Interface(t) | Member::Module(t) => t.line,
        Member::TypeDef { line, .. } => *line,
    }
}

fn struct_member_line(m: &StructMember) -> usize {
    match m {
        StructMember::Field(d) => d.line,
        StructMember::Method(f) => f.sig.line,
        StructMember::Implement { line, .. } => *line,
    }
}

/// A single-line query range `[sc, ec)` on `line`.
#[derive(Clone, Copy)]
struct Q {
    line: usize,
    sc: usize,
    ec: usize,
}

impl Q {
    fn hit<'a>(self, node: AstRef<'a>) -> Option<AstRef<'a>> {
        node.span().contains(self.line, self.sc, self.ec).then_some(node)
    }
}

/// Finds the most specific (deepest) AST node whose span contains the
/// single-line range `[start_col, end_col)` on `line` — a point query if
/// `start_col == end_col`.
pub fn find_node_in(
    program: &Program,
    line: usize,
    start_col: usize,
    end_col: usize,
) -> Option<AstRef<'_>> {
    let q = Q {
        line,
        sc: start_col,
        ec: end_col,
    };
    program.libraries.iter().find_map(|lib| {
        lib.members
            .iter()
            .find_map(|m| in_member(m, q))
            .or_else(|| q.hit(AstRef::Library(lib)))
    })
}

/// Finds the most specific AST node at a single position.
pub fn find_node_at(program: &Program, line: usize, col: usize) -> Option<AstRef<'_>> {
    find_node_in(program, line, col, col)
}

fn in_member(m: &Member, q: Q) -> Option<AstRef<'_>> {
    let child = match m {
        Member::Var(d) => in_exprs(d.exprs(), q),
        Member::Function(f) => in_function(f, q),
        Member::Struct(s) => s
            .size
            .iter()
            .find_map(|e| in_expr(e, q))
            .or_else(|| s.members.iter().find_map(|sm| in_struct_member(sm, q))),
        Member::Interface(t) | Member::Module(t) => {
            t.members.iter().find_map(|sm| in_struct_member(sm, q))
        }
        Member::TypeDef { size, .. } => size.iter().find_map(|e| in_expr(e, q)),
    };
    child.or_else(|| q.hit(AstRef::Member(m)))
}

fn in_struct_member(m: &StructMember, q: Q) -> Option<AstRef<'_>> {
    let child = match m {
        StructMember::Field(d) => in_exprs(d.exprs(), q),
        StructMember::Method(f) => in_function(f, q),
        StructMember::Implement { .. } => None,
    };
    child.or_else(|| q.hit(AstRef::StructMember(m)))
}

fn in_function(f: &FunctionDecl, q: Q) -> Option<AstRef<'_>> {
    f.body.iter().flatten().find_map(|s| in_stmt(s, q))
}

fn in_stmt(s: &Stmt, q: Q) -> Option<AstRef<'_>> {
    in_exprs(s.exprs(), q)
        .or_else(|| {
            s.bodies()
                .into_iter()
                .find_map(|body| body.iter().find_map(|st| in_stmt(st, q)))
        })
        .or_else(|| q.hit(AstRef::Stmt(s)))
}

fn in_exprs(exprs: Vec<&Expr>, q: Q) -> Option<AstRef<'_>> {
    exprs.into_iter().find_map(|e| in_expr(e, q))
}

fn in_expr(e: &Expr, q: Q) -> Option<AstRef<'_>> {
    let child = match &e.kind {
        ExprKind::AnonFunction(f) => in_function(f, q),
        _ => in_exprs(e.children(), q),
    };
    child.or_else(|| q.hit(AstRef::Expr(e)))
}

#[cfg(test)]
#[path = "query.test.rs"]
mod tests;
