use super::*;
#[derive(Deserialize)]
struct Reference {
    which: usize,
    damping: f64,
    z: Vec<f64>,
    fr: Vec<f64>,
    fi: Vec<f64>,
    nodes: Vec<f64>,
    values_re: Vec<f64>,
    values_im: Vec<f64>,
    weights_re: Vec<f64>,
    weights_im: Vec<f64>,
    x: Vec<f64>,
    expected_re: Vec<f64>,
    expected_im: Vec<f64>,
    max_error: f64,
}
#[test]
fn undamped_damped_and_uniform_lawson_agree_with_independent_octave_svd() {
    on_pool(|| {
        let refs: Vec<Reference> =
            serde_json::from_str(include_str!("fixtures/lawson-damping-reference.json")).unwrap();
        for r in refs {
            let mut project = Project {
                globals: "target(x)=x".into(),
                ..Project::default()
            };
            project.fit.function = "target".into();
            project.fit.lawson = "5".into();
            project.fit.lawson_damping = r.damping.to_string();
            let req = Request::new(&project).unwrap();
            let samples: Vec<_> =
                r.z.iter()
                    .enumerate()
                    .map(|(i, &x)| Sample {
                        x,
                        value: C::new(r.fr[i], r.fi[i]),
                    })
                    .collect();
            let nodes = r
                .nodes
                .iter()
                .enumerate()
                .map(|(i, &x)| Node {
                    row: i,
                    z: C::new(x, 0.),
                    value: C::new(r.values_re[i], r.values_im[i]),
                    weight: C::new(r.weights_re[i], r.weights_im[i]),
                })
                .collect();
            let fit = lawson(
                &req,
                &samples,
                model(nodes).unwrap(),
                0,
                &AtomicU64::new(0),
                &mut |_| {},
            )
            .unwrap();
            for (i, &x) in r.x.iter().enumerate() {
                let expected = C::new(r.expected_re[i], r.expected_im[i]);
                let got = fit.evaluate(C::new(x, 0.)).unwrap();
                assert!(
                    (got - expected).norm() < 1e-8 * (1. + expected.norm()),
                    "case {}, damping {} at {x}: {got} vs {expected}",
                    r.which,
                    r.damping
                );
            }
            let max_error = error(&fit, &samples).0;
            assert!(
                (max_error - r.max_error).abs() < 1e-8,
                "case {}, damping {}: max error {max_error} vs {}",
                r.which,
                r.damping,
                r.max_error
            );
        }
    });
}
