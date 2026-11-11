use super::*;
#[test]
fn both_fit_methods_accept_multiline_real_and_complex_functions() {
    for method in [Method::Classic, Method::Nonlinear] {
        for mode in [Mode::Real, Mode::Complex] {
            let mut p = Project::default();
            p.settings.mode = mode;
            p.globals.push(crate::model::Definition{kind:crate::model::DefinitionKind::NumericFunction,name:"target".into(),parameters:"x".into(),
                expression:format!("let z={};\nlet term=1; let total=1;\nfor k in 1..=20 {{ term *= z/k; total += term; }}\nif abs(z)<2 {{ total }} else {{ exp(z) }}",if mode==Mode::Real {"x"}else{"i*x"})});
            p.fit.function = "target".into();
            p.fit.method = method;
            p.fit.tolerance = "1e-10".into();
            p.fit.max_nodes = "20".into();
            p.fit.cleanup = false;
            let req = Request::new(&p).unwrap();
            let fit = fit(&req, 0, &AtomicU64::new(0), |_| {}).unwrap().unwrap();
            assert!(fit.converged, "{method}, {mode}: {}", fit.note);
            let model = model(fit.nodes).unwrap();
            for j in 0..31 {
                let x = -0.99 + 1.98 * j as f64 / 30.;
                let expected = if mode == Mode::Real {
                    C::new(x.exp(), 0.)
                } else {
                    C::new(0., x).exp()
                };
                assert!((model.evaluate(C::new(x, 0.)).unwrap() - expected).norm() < 1e-8);
            }
        }
    }
}
