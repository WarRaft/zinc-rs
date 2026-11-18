use crate::ast::*;
pub use parser_rs::Node;

/// Builds a visualizable tree out of a parsed program.
pub fn build_tree(program: &Program) -> Node {
    Node::with_children(
        "Program",
        program.libraries.iter().map(library_node).collect(),
    )
    .tag("root", None)
}

/// Renders the program's AST as an indented tree.
pub fn render_tree(program: &Program) -> String {
    parser_rs::render_tree(&build_tree(program))
}

/// Renders the program's AST as a Graphviz DOT graph.
pub fn render_dot(program: &Program) -> String {
    parser_rs::render_dot(&build_tree(program))
}

fn line_span(line: usize) -> Option<Span> {
    Some(Span::whole_line(line))
}

fn library_node(lib: &Library) -> Node {
    let requires = if lib.requires.is_empty() {
        String::new()
    } else {
        let list = lib
            .requires
            .iter()
            .map(|r| {
                if r.optional {
                    format!("optional {}", r.name)
                } else {
                    r.name.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(" requires {list}")
    };
    Node::with_children(
        format!("Library {}{requires} (line {})", lib.name, lib.line),
        lib.members.iter().map(member_node).collect(),
    )
    .tag("decl", line_span(lib.line))
}

fn modifiers_label(m: &Modifiers) -> String {
    let mut s = String::new();
    match m.access {
        Access::Private => s.push_str("private "),
        Access::Public => s.push_str("public "),
        Access::Default => {}
    }
    for (on, word) in [
        (m.is_optional, "optional "),
        (m.is_stub, "stub "),
        (m.is_delegate, "delegate "),
        (m.is_readonly, "readonly "),
        (m.is_static, "static "),
        (m.is_constant, "constant "),
    ] {
        if on {
            s.push_str(word);
        }
    }
    s
}

fn member_node(m: &Member) -> Node {
    match m {
        Member::Var(d) => var_node("Global", d),
        Member::Function(f) => function_node("Function", f),
        Member::Struct(s) => {
            let mut label = format!("Struct {}{}", modifiers_label(&s.modifiers), s.name);
            if let Some(ext) = &s.extends {
                label.push_str(&format!(" extends {ext}"));
            }
            label.push_str(&format!(" (line {})", s.line));
            let mut children = Vec::new();
            if let Some(size) = &s.size {
                children.push(container("Size", s.line, vec![expr_node(size)]));
            }
            children.extend(s.members.iter().map(struct_member_node));
            Node::with_children(label, children).tag("decl", line_span(s.line))
        }
        Member::Interface(t) => type_body_node("Interface", t),
        Member::Module(t) => type_body_node("Module", t),
        Member::TypeDef {
            modifiers,
            name,
            extends,
            size,
            line,
        } => Node::with_children(
            format!(
                "Type {}{name} extends {extends} (line {line})",
                modifiers_label(modifiers)
            ),
            size.iter().map(expr_node).collect(),
        )
        .tag("decl", line_span(*line)),
    }
}

fn type_body_node(keyword: &str, t: &TypeBody) -> Node {
    Node::with_children(
        format!(
            "{keyword} {}{} (line {})",
            modifiers_label(&t.modifiers),
            t.name,
            t.line
        ),
        t.members.iter().map(struct_member_node).collect(),
    )
    .tag("decl", line_span(t.line))
}

fn struct_member_node(m: &StructMember) -> Node {
    match m {
        StructMember::Field(d) => var_node("Field", d),
        StructMember::Method(f) => function_node("Method", f),
        StructMember::Implement {
            name,
            optional,
            line,
        } => Node::leaf(format!(
            "Implement {}module {name} (line {line})",
            if *optional { "optional " } else { "" }
        ))
        .tag("decl", line_span(*line)),
    }
}

fn var_node(keyword: &str, d: &VarDecl) -> Node {
    let dims: String = d
        .dims
        .iter()
        .map(|dim| if dim.is_some() { "[_]" } else { "[]" })
        .collect();
    let label = format!(
        "{keyword} {}{} {}{dims} (line {})",
        modifiers_label(&d.modifiers),
        d.type_name,
        d.name,
        d.line
    );
    let mut children = Vec::new();
    for dim in d.dims.iter().flatten() {
        children.push(container("Size", d.line, vec![expr_node(dim)]));
    }
    if let Some(init) = &d.initializer {
        children.push(expr_node(init));
    }
    let kind = if keyword == "Local" { "stmt" } else { "decl" };
    Node::with_children(label, children).tag(kind, line_span(d.line))
}

fn sig_label(sig: &FunctionSig) -> String {
    let params = sig
        .params
        .iter()
        .map(|p| format!("{} {}", p.type_name, p.name))
        .collect::<Vec<_>>()
        .join(", ");
    let returns = sig
        .returns
        .as_deref()
        .map(|r| format!(" -> {r}"))
        .unwrap_or_default();
    format!(
        "{}{}({params}){returns} (line {})",
        modifiers_label(&sig.modifiers),
        sig.name,
        sig.line
    )
}

fn function_node(keyword: &str, f: &FunctionDecl) -> Node {
    let children = f
        .body
        .iter()
        .flatten()
        .map(stmt_node)
        .collect::<Vec<_>>();
    Node::with_children(format!("{keyword} {}", sig_label(&f.sig)), children)
        .tag("sig", line_span(f.sig.line))
}

fn container(label: &str, line: usize, children: Vec<Node>) -> Node {
    Node::with_children(label, children).tag("container", line_span(line))
}

fn stmts_container(label: &str, line: usize, body: &[Stmt]) -> Node {
    container(label, line, body.iter().map(stmt_node).collect())
}

fn stmt_node(s: &Stmt) -> Node {
    let line = s.line();
    let (label, children) = match s {
        Stmt::Local(d) => return var_node("Local", d),
        Stmt::Assign {
            target, op, value, ..
        } => (
            format!("Assign {op:?} (line {line})"),
            vec![
                container("Target", line, vec![expr_node(target)]),
                container("Value", line, vec![expr_node(value)]),
            ],
        ),
        Stmt::Expr { expr, .. } => (format!("Expr (line {line})"), vec![expr_node(expr)]),
        Stmt::If {
            is_static,
            cond,
            then_body,
            else_body,
            ..
        } => {
            let mut children = vec![
                container("Condition", line, vec![expr_node(cond)]),
                stmts_container("Then", line, then_body),
            ];
            if let Some(body) = else_body {
                children.push(stmts_container("Else", line, body));
            }
            let keyword = if *is_static { "Static If" } else { "If" };
            (format!("{keyword} (line {line})"), children)
        }
        Stmt::While { cond, body, .. } => (
            format!("While (line {line})"),
            vec![
                container("Condition", line, vec![expr_node(cond)]),
                stmts_container("Body", line, body),
            ],
        ),
        Stmt::DoWhile { body, cond, .. } => (
            format!("DoWhile (line {line})"),
            vec![
                stmts_container("Body", line, body),
                container("Condition", line, vec![expr_node(cond)]),
            ],
        ),
        Stmt::For {
            start,
            start_op,
            var,
            end_op,
            end,
            body,
            ..
        } => (
            format!("For {var} (line {line})"),
            vec![
                container(&format!("Start {start_op:?}"), line, vec![expr_node(start)]),
                container(&format!("End {end_op:?}"), line, vec![expr_node(end)]),
                stmts_container("Body", line, body),
            ],
        ),
        Stmt::Break { .. } => (format!("Break (line {line})"), Vec::new()),
        Stmt::Return { value, .. } => (
            format!("Return (line {line})"),
            value.iter().map(expr_node).collect(),
        ),
        Stmt::Debug { body, .. } => (
            format!("Debug (line {line})"),
            body.iter().map(stmt_node).collect(),
        ),
        Stmt::Block { body, .. } => (
            format!("Block (line {line})"),
            body.iter().map(stmt_node).collect(),
        ),
    };
    Node::with_children(label, children).tag("stmt", line_span(line))
}

fn expr_node(e: &Expr) -> Node {
    let span = Some(e.span);
    match &e.kind {
        ExprKind::IntLiteral(v) => Node::leaf(format!("Int {v}")).tag("lit", span),
        ExprKind::RealLiteral(v) => Node::leaf(format!("Real {v}")).tag("lit", span),
        ExprKind::StringLiteral(v) => Node::leaf(format!("String {v:?}")).tag("lit", span),
        ExprKind::BoolLiteral(v) => Node::leaf(format!("Bool {v}")).tag("lit", span),
        ExprKind::Null => Node::leaf("Null").tag("lit", span),
        ExprKind::This => Node::leaf("This").tag("expr", span),
        ExprKind::ThisType => Node::leaf("ThisType").tag("expr", span),
        ExprKind::Var(name) => Node::leaf(format!("Var {name}")).tag("expr", span),
        ExprKind::Member(obj, name) => {
            Node::with_children(format!("Member {name}"), vec![expr_node(obj)]).tag("expr", span)
        }
        ExprKind::Index(arr, idx) => {
            Node::with_children("Index", vec![expr_node(arr), expr_node(idx)]).tag("expr", span)
        }
        ExprKind::Call(callee, args) => {
            // `Call foo` / `Call this.bar` for plain paths; anything fancier
            // keeps the callee as its own first child.
            let (label, mut children) = match callee.path() {
                Some(path) => (format!("Call {path}"), Vec::new()),
                None => ("Call".to_string(), vec![expr_node(callee)]),
            };
            children.extend(args.iter().map(expr_node));
            Node::with_children(label, children).tag("expr", span)
        }
        ExprKind::FuncRef(name) => Node::leaf(format!("FuncRef {name}")).tag("expr", span),
        ExprKind::AnonFunction(f) => {
            let mut node = function_node("AnonFunction", f);
            node.span = span;
            node
        }
        ExprKind::Unary(op, inner) => {
            Node::with_children(format!("Unary {op:?}"), vec![expr_node(inner)]).tag("expr", span)
        }
        ExprKind::Binary(l, op, r) => {
            Node::with_children(format!("Binary {op:?}"), vec![expr_node(l), expr_node(r)])
                .tag("expr", span)
        }
    }
}

#[cfg(test)]
#[path = "printer.test.rs"]
mod tests;
