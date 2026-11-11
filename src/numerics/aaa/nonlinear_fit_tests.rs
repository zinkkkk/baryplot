use super::*;
fn project(body: &str, mode: Mode) -> Project {
    let mut p = Project {
        globals: format!("target(x)={body}").into(),
        ..Project::default()
    };
    p.settings.mode = mode;
    p.fit.function = "target".into();
    p.fit.method = Method::Nonlinear;
    p.fit.cleanup = false;
    p.fit.tolerance = "1e-10".into();
    p.fit.max_nodes = "20".into();
    p
}
#[test]
fn refined_continuum_fits_real_complex_and_constant_functions() {
    for (body, mode) in [
        ("0", Mode::Real),
        ("2", Mode::Real),
        ("exp(x)", Mode::Real),
        ("1/(x-2)", Mode::Real),
        ("exp(i*x)/(x-2)", Mode::Complex),
    ] {
        let req = Request::new(&project(body, mode)).unwrap();
        let fit = fit(&req, 0, &AtomicU64::new(0), |_| {}).unwrap().unwrap();
        assert!(
            fit.converged,
            "{body}: {:.3e}, {}",
            fit.relative_error, fit.note
        );
        assert_eq!(fit.method, Method::Nonlinear);
        assert!(fit.relative_l2 < 1e-9);
        let model = model(fit.nodes).unwrap();
        for i in 0..113 {
            let x = -1. + 2. * (i as f64 + 0.37) / 113.;
            let value = model.evaluate(C::new(x, 0.)).unwrap();
            let expected = req.value(x).unwrap();
            assert!(
                (value - expected).norm() < 1e-8 * (1. + expected.norm()),
                "{body} at {x}"
            );
        }
    }
}
#[test]
fn budgets_and_optional_damped_lawson_stay_bounded() {
    let mut p = project("abs(x)+0.1", Mode::Real);
    p.fit.fixed_degree = true;
    p.fit.degree = "9".into();
    p.fit.lawson = "3".into();
    p.fit.lawson_damping = "0.4".into();
    let req = Request::new(&p).unwrap();
    let mut lawson_steps = 0;
    let mut refinements = 0;
    let fit = fit(&req, 0, &AtomicU64::new(0), |progress| match progress {
        Progress::Lawson { step, .. } => lawson_steps = lawson_steps.max(step),
        Progress::Refining { .. } => refinements += 1,
        _ => {}
    })
    .unwrap()
    .unwrap();
    assert_eq!(fit.nodes.len(), 10);
    assert_eq!(lawson_steps, 3);
    assert!(refinements > 0);
    assert_eq!(fit.history.last().unwrap().nodes, 10);
    assert!(
        fit.history[0].grid < 50,
        "continuum fitting must start with local probes"
    );
    assert!(
        fit.history.windows(2).all(|p| p[1].grid >= p[0].grid),
        "NL-AAA must retain previously learned probes"
    );
    assert!(fit.relative_l2.is_finite());
}
#[test]
fn method_options_validate_only_when_used_and_old_projects_keep_classic_defaults() {
    let mut p = project("x", Mode::Real);
    p.fit.method = Method::Classic;
    p.fit.sk_steps = "invalid".into();
    p.fit.lawson_damping = "invalid".into();
    assert!(Request::new(&p).is_ok());
    p.fit.method = Method::Nonlinear;
    assert!(Request::new(&p).unwrap_err().contains("SK iterations"));
    p.fit.sk_steps = "20".into();
    p.fit.lawson = "2".into();
    assert!(Request::new(&p).unwrap_err().contains("Lawson damping"));
    for d in ["-0.1", "1.1", "i"] {
        p.fit.lawson_damping = d.into();
        assert!(Request::new(&p).is_err());
    }
    for d in ["0", "0.4", "1"] {
        p.fit.lawson_damping = d.into();
        assert!(Request::new(&p).is_ok());
    }
    let legacy: Settings = serde_json::from_str(r#"{"function":"target","lawson":"3"}"#).unwrap();
    assert_eq!(legacy.method, Method::Classic);
    assert_eq!(legacy.lawson_damping, "1");
    p.fit.lawson_damping = "0.3".into();
    let restored: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(restored, p);
}
#[test]
#[ignore = "bounded CPU comparison of AAA and NL-AAA on sharp functions"]
fn benchmark_refined_functions() {
    for body in [
        "max(x,0)+0.1",
        "abs(sin(3*pi*x))+0.2",
        "2*abs(3*x-floor(3*x+0.5))+0.1",
    ] {
        for method in [Method::Classic, Method::Nonlinear] {
            let mut p = project(body, Mode::Real);
            p.fit.method = method;
            p.fit.fixed_degree = true;
            p.fit.degree = "23".into();
            let req = Request::new(&p).unwrap();
            let f = fit(&req, 0, &AtomicU64::new(0), |_| {}).unwrap().unwrap();
            eprintln!(
                "{method}: {body}, {} nodes, {:.1} ms, L2 {:.3e}, max {:.3e}, {} evaluations",
                f.nodes.len(),
                f.elapsed_ms,
                f.relative_l2,
                f.relative_error,
                f.evaluations
            );
        }
    }
}
