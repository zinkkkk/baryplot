use super::*;
#[test]
fn data_rows_require_fields_and_function_arguments_remain_scalar() {
    for mode in [Mode::Real, Mode::Complex] {
        let mut p = crate::model::reference_project(crate::model::Mode::Real);
        p.settings.mode = mode;
        let c = p.compile().expect("project");
        for source in ["p1", "p1+1", "re(p1)", "p1[im]"] {
            let error = expression::evaluate(source, &c.variables).expect_err(source);
            assert!(
                error.contains("is a data row") && error.contains("p1[v]"),
                "{source}: {error}"
            );
        }
        assert_eq!(
            expression::evaluate("p1[v]", &c.variables).expect("explicit field"),
            C::new(1., 0.)
        );
        assert!(
            crate::sampling::real("p1", &c)
                .expect_err("settings also require fields")
                .contains("select")
        );
        p.entries[0].expressions[1] = "p2".into();
        assert!(
            p.compile()
                .expect_err("bare row in input")
                .contains("p2[v]")
        );
        p.globals = "a=2\nf(x)=p1".into();
        assert!(
            p.resolve_definitions().issues[&DefinitionKey::GlobalExpression(1)].contains("p1[v]")
        );
        // A scalar argument may share a row's name; it never denotes the data row.
        p.globals = "a=2\nf(p1)=p1+1".into();
        p.entries[0].expressions[1] = "f(p2[v])".into();
        assert_eq!(
            p.compile().expect("local argument").nodes[0].value,
            C::new(0., 0.)
        );
    }
}
#[test]
fn forward_globals_functions_and_row_selectors_share_live_dependencies() {
    let mut p = Project {
        globals: "bias = p3[n] + 1\nf(z) = g(z) + bias\ng(z) = z^2\na = 2".into(),
        ..crate::model::reference_project(crate::model::Mode::Real)
    };
    p.entries[0].expressions[1] = "f(p2[v])".into();
    p.entries[0].expressions[2] = "2.7".into();
    p.entries[0].locks[2][0] = true;
    p.entries[1].expressions[1] = "3".into();
    p.entries[2].expressions[1] = "p1[v] + p1[w]".into();
    let c = p.compile().expect("forward dependencies");
    assert_eq!(c.nodes[0].value, C::new(12., 0.));
    assert_eq!(c.nodes[2].value, C::new(15., 0.));
    for (source, value) in [
        ("p1[n]", -2.),
        ("p1[v]", 12.),
        ("p1[w]", 3.),
        ("p1[wv]", 36.),
        ("p1[s]", 36.),
        ("p1[3]", 36.),
        ("p1.node", -2.),
        ("p1.value", 12.),
        ("p1.weight", 3.),
        ("p1[0]", -2.),
        ("p1[1]", 12.),
        ("p1[2]", 3.),
        ("p1[n][im]", 0.),
    ] {
        assert_eq!(
            expression::evaluate(source, &c.variables).expect(source),
            C::new(value, 0.),
            "{source}"
        );
    }
    assert_eq!(
        crate::sampling::real("f(2)", &c).expect("settings support functions"),
        7.
    );
    p.entries[1].expressions[1] = "4".into();
    let c = p.compile().expect("live dependency");
    assert_eq!(c.nodes[0].value, C::new(19., 0.));
    assert_eq!(c.nodes[2].value, C::new(22., 0.));
}
#[test]
fn complex_components_resolve_independently_and_publish_snapped_values() {
    let mut p = crate::model::reference_project(crate::model::Mode::Real);
    p.settings.mode = Mode::Complex;
    p.split_components();
    p.entries[0].expressions[1] = "p2[v][im]+1".into();
    p.entries[0].imaginary.as_mut().expect("components")[1] = "3".into();
    p.entries[0].imaginary.as_mut().expect("components")[0] = "p1[v][im]".into();
    p.entries[0].imaginary.as_mut().expect("components")[2] = "2".into();
    p.entries[1].imaginary.as_mut().expect("components")[1] = "4.6".into();
    p.entries[1].locks[1][1] = true;
    p.entries[2].expressions[1] = "p1[wv][re]".into();
    p.entries[2].imaginary.as_mut().expect("components")[1] = "p1[n][im]".into();
    let c = p.compile().expect("cross-component references");
    assert_eq!(c.nodes[0].value, C::new(6., 3.));
    assert_eq!(c.nodes[0].z, C::new(-2., 3.));
    assert_eq!(c.nodes[2].value, C::new(0., 3.));
    assert_eq!(
        expression::evaluate("p1[wv]", &c.variables).expect("product"),
        C::new(0., 15.)
    );
    assert_eq!(
        expression::evaluate("p1[n][im]", &c.variables).expect("imag"),
        C::new(3., 0.)
    );
    p.settings.mode = Mode::Real;
    let c = p.compile().expect("real projection");
    assert_eq!(c.nodes[0].value, C::new(1., 0.));
    assert_eq!(c.nodes[2].value, C::new(1., 0.));
}
#[test]
fn cycles_bad_signatures_unknown_names_and_collisions_are_local_errors() {
    for source in [
        "a=b\nb=a",
        "a=2\nf(x)=g(x)\ng(x)=f(x)",
        "a=f(1)\nf(x)=x+a",
        "a=2\nf(x,x)=x",
        "a=2\nf(x)=absent+x",
        "a=2\nf(x)=sin(x,2)",
    ] {
        let p = Project {
            globals: source.into(),
            ..crate::model::reference_project(crate::model::Mode::Real)
        };
        assert!(p.compile().is_err(), "{source}");
        let resolved = p.resolve_definitions();
        assert!(
            resolved.issues.keys().any(|k| matches!(
                k,
                DefinitionKey::GlobalExpression(_) | DefinitionKey::GlobalParameters(_)
            )),
            "{source}"
        );
    }
    let mut p = crate::model::reference_project(crate::model::Mode::Real);
    p.entries[0].expressions[1] = "p2[v]".into();
    p.entries[1].expressions[1] = "p1[v]".into();
    let r = p.resolve_definitions();
    for row in [0, 1] {
        assert!(r.issues[&DefinitionKey::Parameter(row, 1, 0)].contains("circular"));
    }
    p.globals = "a=2\np1=4\na=3".into();
    let r = p.resolve_definitions();
    for key in [
        DefinitionKey::GlobalName(0),
        DefinitionKey::GlobalName(1),
        DefinitionKey::GlobalName(2),
        DefinitionKey::RowName(0),
    ] {
        assert!(r.issues.contains_key(&key), "{key:?}");
    }
}
#[test]
fn legacy_definitions_migrate_to_a_table_and_user_functions_round_trip() {
    let mut legacy: serde_json::Value =
        serde_json::from_str(include_str!("../../../examples/real.json")).expect("project JSON");
    legacy["globals"] = serde_json::Value::String("a=2".into());
    let mut p: Project = serde_json::from_value(legacy).expect("legacy string definitions");
    assert_eq!(p.globals.len(), 1);
    p.globals.push(Definition {
        kind: DefinitionKind::Function,
        name: "wave".into(),
        parameters: "x, phase".into(),
        expression: "a*sin(x+phase)".into(),
    });
    p.entries[0].expressions[1] = "wave(pi/2,0)".into();
    let json = serde_json::to_string(&p).expect("serialize");
    assert!(serde_json::from_str::<serde_json::Value>(&json).expect("json")["globals"].is_array());
    let loaded: Project = serde_json::from_str(&json).expect("load table");
    assert_eq!(loaded.globals, p.globals);
    assert_eq!(
        loaded.compile().expect("function after load").nodes[0].value,
        C::new(2., 0.)
    );
}
