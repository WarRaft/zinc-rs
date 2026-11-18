use zinc_rs::ast::Member;
use zinc_rs::{lint, parse};

const SAMPLE: &str = include_str!("fixtures/sample.zn");

#[test]
fn sample_file_parses_successfully() {
    let program = parse(SAMPLE).expect("sample.zn should parse without errors");
    assert_eq!(program.libraries.len(), 1);
    let lib = &program.libraries[0];
    assert_eq!(lib.name, "Counters");

    let function_names: Vec<&str> = lib
        .members
        .iter()
        .filter_map(|m| match m {
            Member::Function(f) => Some(f.sig.name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(function_names, vec!["IsEven", "SumCounters", "onInit"]);
}

#[test]
fn sample_file_has_no_lint_diagnostics() {
    let program = parse(SAMPLE).expect("sample.zn should parse without errors");
    let diagnostics = lint(&program);
    assert!(
        diagnostics.is_empty(),
        "expected sample.zn to be clean, got {diagnostics:?}"
    );
}

#[test]
fn reports_parse_error_with_location() {
    let err = parse("library L {\n  function F() {\n    x = ;\n  }\n}").unwrap_err();
    assert_eq!((err.line, err.col), (3, 9));
}

#[test]
fn lint_catches_multiple_issues_at_once() {
    let src = r#"
library L {
    integer x = 1;
    integer x = 2;

    function Foo() -> integer {
        integer unused = 5;
        while (true) {
            DoSomething();
        }
    }
}
"#;
    let program = parse(src).expect("should still parse despite lint issues");
    let diagnostics = lint(&program);
    for needle in [
        "duplicate library member 'x'",
        "unused local variable 'unused'",
        "always true",
        "no 'return' statement",
    ] {
        assert!(
            diagnostics.iter().any(|d| d.message.contains(needle)),
            "missing {needle:?} in {diagnostics:?}"
        );
    }
}
