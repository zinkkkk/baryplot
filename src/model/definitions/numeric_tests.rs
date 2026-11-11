use super::*;
#[test]
fn numeric_functions_resolve_forward_dependencies_and_round_trip_in_projects() {
    let mut project = crate::model::reference_project(Mode::Real);
    for (entry, x) in project.entries.iter_mut().zip(["-2", "0", "2"]) {
        entry.expressions[0] = x.into();
    }
    project.globals = "bias=p3[n]+1\nhelper(z)=z^2".into();
    project.globals.push(Definition{kind:DefinitionKind::NumericFunction,name:"target".into(),parameters:"x".into(),
        expression:"let total=bias;\nfor k in 1..=3 { total += helper(x)/k; }\nif re(x)<0 { -total } else { total }".into()});
    project.entries[0].expressions[1] = "target(2)".into();
    project.entries[1].expressions[1] = "target(-2)".into();
    let model = project.compile().unwrap();
    let expected = 3. + 4. * (1. + 0.5 + 1. / 3.);
    assert!((model.nodes[0].value.re - expected).abs() < 1e-12);
    assert!((model.nodes[1].value.re + expected).abs() < 1e-12);
    assert_eq!(
        expression::evaluate("target(0)", &model.variables).unwrap(),
        C::new(3., 0.)
    );
    assert!(!model.variables.contains_key("total"));
    let json = serde_json::to_string(&project).unwrap();
    assert_eq!(serde_json::from_str::<Project>(&json).unwrap(), project);
    let binary = rmp_serde::to_vec_named(&project).unwrap();
    assert_eq!(rmp_serde::from_slice::<Project>(&binary).unwrap(), project);
    project.globals[1].expression = "2*z^2".into();
    assert!((project.compile().unwrap().nodes[0].value.re - (2. * expected - 3.)).abs() < 1e-12);
}
#[test]
fn unused_branches_still_check_names_arities_and_recursive_dependencies() {
    let mut project = Project::default();
    project.globals.push(Definition {
        kind: DefinitionKind::NumericFunction,
        name: "target".into(),
        parameters: "x".into(),
        expression: String::new(),
    });
    for (source, expected) in [
        ("if false {target(x)} else {x}", "Recursive"),
        (
            "let y=0; for k in 0..2 {y+=unknown(x);} y",
            "Unknown function",
        ),
        ("if true {x} else {sin(1,2)}", "expects"),
        ("let y=2; y+missing", "Unknown variable"),
        (
            "let y=0; for k in 0..3 {let hidden=1;} hidden",
            "Unknown variable",
        ),
    ] {
        project.globals[0].expression = source.into();
        let resolved = project.resolve_definitions();
        let error = &resolved.issues[&DefinitionKey::GlobalExpression(0)];
        assert!(error.contains(expected), "{source}: {error}");
    }
}
