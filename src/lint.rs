use crate::ast::*;
use std::collections::{HashMap, HashSet};

pub use parser_rs::{Diagnostic, Level};
use parser_rs::{rule, Fix};

/// Runs every lint rule over a parsed program and returns the diagnostics
/// found, sorted by source line.
pub fn lint(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    check_duplicates(
        "library",
        program.libraries.iter().map(|l| (l.name.as_str(), l.line)),
        &mut diags,
    );

    let mut functions = Vec::new();
    // The global scope is checked like one more library body.
    check_duplicates("global", program.globals.iter().filter_map(member_name), &mut diags);
    for m in &program.globals {
        if let Some(word) = library_only_access(m) {
            diags.push(Diagnostic::warning(rule::MODIFIER_OUTSIDE_LIBRARY, 
                format!("'{word}' has no effect outside a library"),
                m.line(),
            ));
        }
    }
    for s in &program.stmts {
        diags.push(Diagnostic::error(rule::STATEMENT_OUTSIDE_FUNCTION, top_level_stmt_message(s), s.line()));
    }
    let bodies = program
        .libraries
        .iter()
        .map(|lib| lib.members.as_slice())
        .chain(std::iter::once(program.globals.as_slice()));
    for lib in &program.libraries {
        check_duplicates(
            "library member",
            lib.members.iter().filter_map(member_name),
            &mut diags,
        );
    }
    for members in bodies {
        for m in members {
            match m {
                Member::Function(f) => functions.push(f),
                Member::Var(d) => collect_anon_functions_in(d.exprs(), &mut functions),
                Member::Struct(s) => struct_members(&s.members, &mut diags, &mut functions),
                Member::Interface(t) | Member::Module(t) => {
                    struct_members(&t.members, &mut diags, &mut functions)
                }
                Member::TypeDef { .. } => {}
            }
        }
    }

    // Anonymous functions are found while checking their enclosing
    // function, and are then checked as functions of their own.
    while let Some(f) = functions.pop() {
        check_function(f, &mut diags, &mut functions);
    }

    diags.sort_by_key(|d| d.line);
    diags
}

/// `private` / `public` on a global-scope declaration, which has no
/// library to be private or public in.
fn library_only_access(m: &Member) -> Option<&'static str> {
    let access = match m {
        Member::Var(d) => d.modifiers.access,
        Member::Function(f) => f.sig.modifiers.access,
        Member::Struct(s) => s.modifiers.access,
        Member::Interface(t) | Member::Module(t) => t.modifiers.access,
        Member::TypeDef { modifiers, .. } => modifiers.access,
    };
    match access {
        Access::Private => Some("private"),
        Access::Public => Some("public"),
        Access::Default => None,
    }
}

/// Why a statement can't stand outside a function.
fn top_level_stmt_message(s: &Stmt) -> String {
    let what = match s {
        Stmt::Local(_) => unreachable!("a declaration outside a function is a global"),
        Stmt::Assign { .. } => "assignment",
        Stmt::Expr { .. } => "call",
        Stmt::If { is_static: true, .. } => "'static if'",
        Stmt::If { .. } => "'if' statement",
        Stmt::While { .. } => "'while' loop",
        Stmt::DoWhile { .. } => "'do ... while' loop",
        Stmt::For { .. } => "'for' loop",
        Stmt::Break { .. } => "'break'",
        Stmt::Return { .. } => "'return'",
        Stmt::Debug { .. } => "'debug' statement",
        Stmt::Block { .. } => "block",
    };
    format!("{what} outside a function: move it into a function")
}

fn member_name(m: &Member) -> Option<(&str, usize)> {
    Some(match m {
        Member::Var(d) => (&d.name, d.line),
        Member::Function(f) => (&f.sig.name, f.sig.line),
        Member::Struct(s) => (&s.name, s.line),
        Member::Interface(t) | Member::Module(t) => (&t.name, t.line),
        Member::TypeDef { name, line, .. } => (name, *line),
    })
}

fn struct_members<'a>(
    members: &'a [StructMember],
    diags: &mut Vec<Diagnostic>,
    functions: &mut Vec<&'a FunctionDecl>,
) {
    check_duplicates(
        "struct member",
        members.iter().filter_map(|m| match m {
            StructMember::Field(d) => Some((d.name.as_str(), d.line)),
            StructMember::Method(f) => Some((f.sig.name.as_str(), f.sig.line)),
            StructMember::Implement { .. } => None,
        }),
        diags,
    );
    for m in members {
        match m {
            StructMember::Method(f) => functions.push(f),
            StructMember::Field(d) => collect_anon_functions_in(d.exprs(), functions),
            StructMember::Implement { .. } => {}
        }
    }
}

fn check_duplicates<'a>(
    what: &str,
    names: impl Iterator<Item = (&'a str, usize)>,
    diags: &mut Vec<Diagnostic>,
) {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for (name, line) in names {
        match seen.get(name) {
            Some(&first) => diags.push(Diagnostic::error(rule::DUPLICATE_DECLARATION, 
                format!("duplicate {what} '{name}' (first declared at line {first})"),
                line,
            )),
            None => {
                seen.insert(name, line);
            }
        }
    }
}

fn check_function<'a>(
    f: &'a FunctionDecl,
    diags: &mut Vec<Diagnostic>,
    functions: &mut Vec<&'a FunctionDecl>,
) {
    let Some(body) = &f.body else {
        return; // an interface method signature
    };

    let mut seen: HashMap<&str, usize> = HashMap::new();
    for p in &f.sig.params {
        seen.insert(&p.name, f.sig.line);
    }

    let mut locals = Vec::new();
    collect_locals(body, &mut locals);

    let mut used = HashSet::new();
    collect_used_vars(body, &mut used);

    for local in locals {
        if let Some(&first) = seen.get(local.name.as_str()) {
            diags.push(Diagnostic::error(rule::DUPLICATE_DECLARATION, 
                format!(
                    "duplicate local variable '{}' (first declared at line {first})",
                    local.name
                ),
                local.line,
            ));
        } else {
            seen.insert(&local.name, local.line);
            if !used.contains(local.name.as_str()) {
                diags.push(Diagnostic::warning(rule::UNUSED_LOCAL, 
                    format!("unused local variable '{}'", local.name),
                    local.line,
                ));
            }
        }
    }

    check_stmts(body, false, diags, functions);

    if f.sig.returns.is_some() && !contains_return(body) {
        let name = if f.sig.name.is_empty() {
            "anonymous function".to_string()
        } else {
            format!("function '{}'", f.sig.name)
        };
        diags.push(Diagnostic::warning(rule::MISSING_RETURN, 
            format!("{name} declares a return type but has no 'return' statement"),
            f.sig.line,
        ));
    }
}

fn collect_locals<'a>(stmts: &'a [Stmt], out: &mut Vec<&'a VarDecl>) {
    for s in stmts {
        if let Stmt::Local(d) = s {
            out.push(d);
        }
        for body in s.bodies() {
            collect_locals(body, out);
        }
    }
}

fn check_stmts<'a>(
    stmts: &'a [Stmt],
    in_loop: bool,
    diags: &mut Vec<Diagnostic>,
    functions: &mut Vec<&'a FunctionDecl>,
) {
    for s in stmts {
        collect_anon_functions_in(s.exprs(), functions);
        match s {
            Stmt::While { cond, body, line } | Stmt::DoWhile { body, cond, line } => {
                if matches!(cond.kind, ExprKind::BoolLiteral(true)) && !exits_loop(body) {
                    diags.push(Diagnostic::warning(rule::INFINITE_LOOP, 
                        "loop condition is always true and the body has no 'break' or 'return'",
                        *line,
                    ));
                }
            }
            Stmt::If {
                then_body,
                else_body,
                line,
                else_span,
                then_end,
                ..
            } => {
                if then_body.is_empty() {
                    diags.push(Diagnostic::warning(rule::EMPTY_BRANCH, "empty 'if' body", *line));
                }
                if else_body.as_ref().is_some_and(|b| b.is_empty()) {
                    let d = Diagnostic::warning(rule::EMPTY_BRANCH, "empty 'else' body", *line);
                    // Marked from `else` to its `}`; the fix deletes that
                    // and what's between it and the `if` body.
                    diags.push(match else_span {
                        Some(s) => d.at(*s).with_fix(Fix::delete("Remove empty 'else'", Span::new(then_end.end_line, then_end.end_col, s.end_line, s.end_col))),
                        None => d,
                    });
                }
            }
            Stmt::Break { line } if !in_loop => {
                diags.push(Diagnostic::error(rule::BREAK_OUTSIDE_LOOP, "'break' outside of a loop", *line));
            }
            _ => {}
        }
        let is_loop = matches!(
            s,
            Stmt::While { .. } | Stmt::DoWhile { .. } | Stmt::For { .. }
        );
        for body in s.bodies() {
            check_stmts(body, in_loop || is_loop, diags, functions);
        }
    }
}

/// Whether `body` (of a loop) contains a `break` that leaves *this* loop
/// (not one belonging to a nested loop) or any `return`.
fn exits_loop(body: &[Stmt]) -> bool {
    body.iter().any(|s| match s {
        Stmt::Break { .. } | Stmt::Return { .. } => true,
        Stmt::While { body, .. } | Stmt::DoWhile { body, .. } | Stmt::For { body, .. } => {
            contains_return(body)
        }
        _ => s.bodies().into_iter().any(exits_loop),
    })
}

fn contains_return(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| {
        matches!(s, Stmt::Return { .. }) || s.bodies().into_iter().any(contains_return)
    })
}

/// Names read anywhere in `stmts`. Plain `x = ...` doesn't read `x`; a
/// compound `x += ...` and a `for` counter do.
fn collect_used_vars<'a>(stmts: &'a [Stmt], used: &mut HashSet<&'a str>) {
    for s in stmts {
        match s {
            Stmt::Assign {
                target,
                op: AssignOp::Assign,
                value,
                ..
            } => {
                if !matches!(target.kind, ExprKind::Var(_)) {
                    collect_used_in_expr(target, used);
                }
                collect_used_in_expr(value, used);
            }
            Stmt::For { var, .. } => {
                used.insert(var);
                s.exprs().into_iter().for_each(|e| collect_used_in_expr(e, used));
            }
            _ => s.exprs().into_iter().for_each(|e| collect_used_in_expr(e, used)),
        }
        for body in s.bodies() {
            collect_used_vars(body, used);
        }
    }
}

fn collect_used_in_expr<'a>(e: &'a Expr, used: &mut HashSet<&'a str>) {
    if let ExprKind::Var(name) = &e.kind {
        used.insert(name);
    }
    for child in e.children() {
        collect_used_in_expr(child, used);
    }
}

fn collect_anon_functions_in<'a>(exprs: Vec<&'a Expr>, out: &mut Vec<&'a FunctionDecl>) {
    for e in exprs {
        if let ExprKind::AnonFunction(f) = &e.kind {
            out.push(f);
        }
        collect_anon_functions_in(e.children(), out);
    }
}

#[cfg(test)]
#[path = "lint.test.rs"]
mod tests;
