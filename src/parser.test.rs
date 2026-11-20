use super::*;

fn parse(src: &str) -> Program {
    Parser::parse_str(src).unwrap_or_else(|e| panic!("should parse: {e:?}"))
}

/// Parses `body` as the body of `function F()` inside a library.
fn fn_body(body: &str) -> Vec<Stmt> {
    let prog = parse(&format!("library L {{ function F() {{ {body} }} }}"));
    match &prog.libraries[0].members[0] {
        Member::Function(f) => f.body.clone().unwrap(),
        other => panic!("expected function, got {other:?}"),
    }
}

fn expr(src: &str) -> Expr {
    match &fn_body(&format!("x = {src};"))[0] {
        Stmt::Assign { value, .. } => value.clone(),
        other => panic!("expected assignment, got {other:?}"),
    }
}

fn var(name: &str) -> Box<Expr> {
    Box::new(Expr::new(ExprKind::Var(name.into()), Span::whole_line(0)))
}

#[test]
fn parses_library_header_with_requirements() {
    let prog = parse("library A requires B, optional C {}\nlibrary D {}");
    assert_eq!(prog.libraries.len(), 2);
    let a = &prog.libraries[0];
    assert_eq!(a.name, "A");
    assert_eq!(
        a.requires,
        vec![
            Requirement {
                name: "B".into(),
                optional: false
            },
            Requirement {
                name: "C".into(),
                optional: true
            },
        ]
    );
    assert_eq!(prog.libraries[1].line, 2);
}

#[test]
fn parses_globals_with_modifiers_arrays_and_multiple_names() {
    let prog = parse(
        "library L {\n constant integer N = 3;\n integer a[N], b[][4];\n private { real x, y = 1.5; }\n}",
    );
    let vars: Vec<&VarDecl> = prog.libraries[0]
        .members
        .iter()
        .map(|m| match m {
            Member::Var(d) => d,
            other => panic!("expected var, got {other:?}"),
        })
        .collect();
    assert_eq!(vars.len(), 5);
    assert!(vars[0].modifiers.is_constant);
    assert_eq!(vars[1].dims.len(), 1);
    assert_eq!(vars[2].dims.len(), 2);
    assert!(vars[2].dims[0].is_none());
    assert_eq!(vars[3].name, "x");
    assert_eq!(vars[3].modifiers.access, Access::Private);
    assert_eq!(vars[4].initializer, Some(Expr::new(ExprKind::RealLiteral(1.5), Span::whole_line(0))));
}

#[test]
fn parses_functions_with_params_and_return_type() {
    let prog = parse("library L { public function Add(integer a, real b) -> real { return a + b; } }");
    match &prog.libraries[0].members[0] {
        Member::Function(f) => {
            assert_eq!(f.sig.name, "Add");
            assert_eq!(f.sig.modifiers.access, Access::Public);
            assert_eq!(f.sig.params.len(), 2);
            assert_eq!(f.sig.returns.as_deref(), Some("real"));
            assert_eq!(f.body.as_ref().unwrap().len(), 1);
        }
        other => panic!("expected function, got {other:?}"),
    }
}

#[test]
fn parses_structs_methods_operators_and_modules() {
    let prog = parse(
        "library L {\n struct S[100] extends array {\n  static integer n;\n  module M;\n  static method create() -> thistype { return 0; }\n  method operator []= (integer i, integer v) {}\n }\n interface I { method run(); }\n module M { method x() {} }\n}",
    );
    let members = &prog.libraries[0].members;
    let Member::Struct(s) = &members[0] else {
        panic!("expected struct, got {:?}", members[0]);
    };
    assert_eq!(s.name, "S");
    assert!(s.size.is_some());
    assert_eq!(s.extends.as_deref(), Some("array"));
    assert!(matches!(&s.members[0], StructMember::Field(d) if d.modifiers.is_static));
    assert!(matches!(&s.members[1], StructMember::Implement { name, .. } if name == "M"));
    assert!(
        matches!(&s.members[2], StructMember::Method(f) if f.sig.returns.as_deref() == Some("thistype"))
    );
    assert!(matches!(&s.members[3], StructMember::Method(f) if f.sig.name == "operator []="));
    let Member::Interface(i) = &members[1] else {
        panic!("expected interface");
    };
    assert!(matches!(&i.members[0], StructMember::Method(f) if f.body.is_none()));
    assert!(matches!(&members[2], Member::Module(m) if m.name == "M"));
}

#[test]
fn parses_control_flow() {
    let body = fn_body(
        "if (a) { b(); } else if (c) d(); else {}\n\
         while (x < 3) x += 1;\n\
         do { y(); } while (z);\n\
         for (10 > i >= 0) { break; }\n\
         static if (DEBUG) {}\n\
         debug { e(); }\n\
         { f(); }\n\
         return;",
    );
    assert!(matches!(&body[0], Stmt::If { else_body: Some(e), .. } if matches!(e[0], Stmt::If { .. })));
    assert!(matches!(&body[1], Stmt::While { body, .. } if body.len() == 1));
    assert!(matches!(&body[2], Stmt::DoWhile { .. }));
    match &body[3] {
        Stmt::For {
            start_op,
            var,
            end_op,
            ..
        } => {
            assert_eq!((*start_op, var.as_str(), *end_op), (BinOp::Gt, "i", BinOp::GtEq));
        }
        other => panic!("expected for, got {other:?}"),
    }
    assert!(matches!(&body[4], Stmt::If { is_static: true, .. }));
    assert!(matches!(&body[5], Stmt::Debug { .. }));
    assert!(matches!(&body[6], Stmt::Block { .. }));
    assert!(matches!(&body[7], Stmt::Return { value: None, .. }));
}

#[test]
fn parses_locals_and_assignments() {
    let body = fn_body("integer i, j = 2; thistype t; a.b[i] -= 1; this.c = j;");
    assert!(matches!(&body[0], Stmt::Local(d) if d.name == "i"));
    assert!(matches!(&body[1], Stmt::Local(d) if d.name == "j" && d.initializer.is_some()));
    assert!(matches!(&body[2], Stmt::Local(d) if d.type_name == "thistype"));
    assert!(matches!(&body[3], Stmt::Assign { op: AssignOp::Sub, target, .. } if matches!(target.kind, ExprKind::Index(..))));
    assert!(matches!(&body[4], Stmt::Assign { op: AssignOp::Assign, target, .. } if target.path().as_deref() == Some("this.c")));
}

#[test]
fn respects_operator_precedence() {
    // JASS precedence: `&&` binds looser than `||`.
    assert_eq!(
        expr("a && b || c").kind,
        ExprKind::Binary(
            var("a"),
            BinOp::And,
            Box::new(Expr::new(
                ExprKind::Binary(var("b"), BinOp::Or, var("c")),
                Span::whole_line(0)
            ))
        )
    );
    assert_eq!(
        expr("a || b && c").kind,
        ExprKind::Binary(
            Box::new(Expr::new(
                ExprKind::Binary(var("a"), BinOp::Or, var("b")),
                Span::whole_line(0)
            )),
            BinOp::And,
            var("c"),
        )
    );
    assert!(matches!(expr("-a % b").kind, ExprKind::Binary(_, BinOp::Mod, _)));
    assert!(matches!(expr("!(a == b)").kind, ExprKind::Unary(UnaryOp::Not, _)));
}

#[test]
fn parses_postfix_chains_and_function_references() {
    let e = expr("Data.create(1)[2].x");
    assert!(matches!(&e.kind, ExprKind::Member(obj, name) if name == "x" && matches!(obj.kind, ExprKind::Index(..))));
    assert_eq!(expr("function Data.onTick").kind, ExprKind::FuncRef("Data.onTick".into()));
    match expr("function(integer a) -> integer { return a; }").kind {
        ExprKind::AnonFunction(f) => {
            assert_eq!(f.sig.params.len(), 1);
            assert_eq!(f.body.unwrap().len(), 1);
        }
        other => panic!("expected anonymous function, got {other:?}"),
    }
}

#[test]
fn records_precise_expression_spans() {
    let prog = parse("library L {\nfunction F() {\n    x = foo.bar(1);\n}\n}");
    let Member::Function(f) = &prog.libraries[0].members[0] else {
        panic!()
    };
    let Stmt::Assign { value, .. } = &f.body.as_ref().unwrap()[0] else {
        panic!()
    };
    assert_eq!(value.span, Span::new(3, 9, 3, 19));
}

#[test]
fn reports_errors_at_the_offending_token() {
    let err = Parser::parse_str("library L {\n  function F() { x = ; }\n}").unwrap_err();
    assert_eq!((err.line, err.col, err.end_col), (2, 22, 23));

    // Outside a library: a statement position, so the same message.
    let err = Parser::parse_str("x;").unwrap_err();
    assert!(err.message.contains("assignment or a call"));

    let err = Parser::parse_str("library L { function F() { x; } }").unwrap_err();
    assert!(err.message.contains("assignment or a call"));

    let err = Parser::parse_str("library L { function F() { for (i < 3) {} } }").unwrap_err();
    assert!(err.message.contains("range loop header"));

    let err = Parser::parse_str("library L { function F() {").unwrap_err();
    assert!(err.message.contains("unexpected end of file"));
}

#[test]
fn parses_the_global_scope() {
    let src = "integer counter = 0;\nlibrary L { integer x; }\nfunction Tick() { counter += 1; }\nprivate { struct S { integer f; } }\nconstant real PI = 3.14;";
    let prog = Parser::parse_str(src).unwrap();
    assert_eq!(prog.libraries.len(), 1);
    let globals: Vec<(usize, bool)> = prog
        .globals
        .iter()
        .map(|m| (m.line(), matches!(m, Member::Var(d) if d.modifiers.is_constant)))
        .collect();
    assert_eq!(globals, [(1, false), (3, false), (4, false), (5, true)]);
    assert!(matches!(&prog.globals[2], Member::Struct(s) if s.modifiers.access == Access::Private));
    assert!(prog.stmts.is_empty());
}

#[test]
fn reads_statements_outside_functions_in_full() {
    let src = "if (x > 0) { f(); } else { g(); }\ncounter = 1;\nstatic if (DEBUG) { h(); }\nfor (0 <= i < 3) { f(); }\nlibrary L {}";
    let prog = Parser::parse_str(src).unwrap();
    let lines: Vec<usize> = prog.stmts.iter().map(Stmt::line).collect();
    assert_eq!(lines, [1, 2, 3, 4]);
    assert!(matches!(&prog.stmts[0], Stmt::If { else_body: Some(_), .. }));
    assert!(matches!(&prog.stmts[2], Stmt::If { is_static: true, .. }));
    assert_eq!(prog.libraries.len(), 1);
}
