use super::*;
fn run(source: &str) -> Result<C, String> {
    Expression::parse_body(source)?.evaluate(&Variables::new())
}
#[test]
fn locals_loops_compound_assignments_and_complex_numbers_compose() {
    for (source, expected) in [
        (
            "let total=0; for k in 1..=20 { total+=k; } total",
            C::new(210., 0.),
        ),
        (
            "let total=0; for k in -2..3 {total += k^2;} total",
            C::new(10., 0.),
        ),
        (
            "let total=2; for k in 3..1 {total=100;} total",
            C::new(2., 0.),
        ),
        (
            "let total=2; for k in 3..3 {total=100;} total",
            C::new(2., 0.),
        ),
        (
            "let total=2; for k in 3..=3 {total+=k;} total",
            C::new(5., 0.),
        ),
        ("let z=1+i; for k in 0..3 {z *= 1+i;} z", C::new(-4., 0.)),
        ("let a=10; a-=2; a/=2; a%=3; a", C::new(1., 0.)),
        (
            "let total=0; for j in 0..3 {for k in 0..=j {total+=1;}} total",
            C::new(6., 0.),
        ),
        (
            "{ // outer braces are optional\nlet x=3; # comment\nx^2\n}",
            C::new(9., 0.),
        ),
        (
            "let total=0; for k in 0..2 {if k==0 {total=2;} else {total+=3;}} total",
            C::new(5., 0.),
        ),
    ] {
        assert_eq!(run(source).expect(source), expected, "{source}");
    }
}
#[test]
fn conditions_and_boolean_operators_are_lazy_and_support_complex_equality() {
    for (source, expected) in [
        ("if true {2} else {1/0}", 2.),
        ("if false {1/0} else if 2>=1 && 3!=4 {7} else {1/0}", 7.),
        ("if false && 1/0 {2} else {3}", 3.),
        ("if true || 1/0 {2} else {3}", 2.),
        ("if !(2<1) && 1<=2 && 3>2 {4} else {5}", 4.),
        ("if 1+i == complex(1,1) {3} else {4}", 3.),
        ("let x=2; if x==2 {x+=3;} x", 5.),
        ("if false {9}", 0.),
    ] {
        assert_eq!(run(source).expect(source), C::new(expected, 0.), "{source}");
    }
    for source in ["if i {1} else {0}", "if i<2 {1} else {0}"] {
        assert!(run(source).unwrap_err().contains("real"));
    }
    assert_eq!(
        evaluate("if 0==0 {42} else {1/0}", &Variables::new()).unwrap(),
        C::new(42., 0.)
    );
}
#[test]
fn lexical_scopes_restore_shadowed_names_and_callees_do_not_capture_caller_locals() {
    assert_eq!(run("let x=2; {let x=10; x+=1;} x").unwrap(), C::new(2., 0.));
    assert_eq!(run("let x=2; {x+=3;} x").unwrap(), C::new(5., 0.));
    assert_eq!(
        run("let k=7; for k in 0..3 {let k=10; k+=1;} k").unwrap(),
        C::new(7., 0.)
    );
    let mut vars = Variables::new();
    vars.insert("x".into(), C::new(10., 0.));
    vars.define_function(
        "read".into(),
        Function {
            parameters: vec![],
            body: Expression::parse("x").unwrap(),
        },
    );
    assert_eq!(
        Expression::parse_body("let x=2; x+read()")
            .unwrap()
            .evaluate(&vars)
            .unwrap(),
        C::new(12., 0.)
    );
    let expr = Expression::parse_body(
        "let x=bias; for k in 0..count {let z=x; x+=helper(z)+p1[v];} x+tail",
    )
    .unwrap();
    let mut globals = BTreeSet::new();
    let mut functions = BTreeSet::new();
    expr.symbols(&mut globals, &mut functions);
    assert_eq!(
        globals,
        BTreeSet::from([
            "bias".into(),
            "count".into(),
            "p1.value".into(),
            "tail".into()
        ])
    );
    assert_eq!(functions, BTreeSet::from(["helper".into()]));
}
#[test]
fn malformed_or_unbounded_bodies_report_locations_and_cannot_write_globals_or_indices() {
    for source in [
        "let x=2;",
        "let x=2 x",
        "let x=2; let x=3; x",
        "x=2; x",
        "for k in 0..3 {k=2;} 0",
        "let sin=2; sin",
        "for k in 0..3 {let x=2;} x=1; x",
        "while true {1} 0",
        "return 2;",
        "let x=1; if false {missing=2;} x",
        "{1",
    ] {
        let error = Expression::parse_body(source).unwrap_err();
        assert!(
            error.contains("line") && error.contains("column"),
            "{source}: {error}"
        );
    }
    for source in [
        "for k in 0..10001 {} 0",
        "for k in 0..0.5 {} 0",
        "for k in 0..i {} 0",
        "for k in 0..1e20 {} 0",
    ] {
        assert!(run(source).unwrap_err().contains("Loop"), "{source}");
    }
    assert!(
        run("for k in 0..10000 {for j in 0..10000 {}} 0")
            .unwrap_err()
            .contains("operation limit")
    );
    let error = run("let x=2;\nlet y=1/0;\nx+y").unwrap_err();
    assert!(error.contains("line 2"), "{error}");
    assert!(Expression::parse_body(&"1".repeat(MAX_BODY_BYTES + 1)).is_err());
    assert!(Expression::parse_body(&format!("{}1{}", "{".repeat(100), "}".repeat(100))).is_err());
    let chain = format!("{}0", "if false {0} else ".repeat(100));
    assert!(Expression::parse_body(&chain).is_err());
}
#[test]
fn nested_numeric_function_calls_share_the_operation_budget() {
    let mut vars = Variables::new();
    vars.define_function(
        "work".into(),
        Function {
            parameters: vec![],
            body: Expression::parse_body("let total=0; for k in 0..1000 {total+=k;} total")
                .unwrap(),
        },
    );
    let expr =
        Expression::parse_body("let total=0; for k in 0..10 {total+=work();} total").unwrap();
    assert!(
        expr.evaluate(&vars)
            .unwrap_err()
            .contains("operation limit")
    );
}
