use super::*;
#[derive(Deserialize)]
struct Reference {
    name: String,
    step: usize,
    z: Vec<f64>,
    fr: Vec<f64>,
    fi: Vec<f64>,
    nodes: Vec<f64>,
    values_re: Vec<f64>,
    values_im: Vec<f64>,
    previous_re: Vec<f64>,
    previous_im: Vec<f64>,
    x: Vec<f64>,
    expected_re: Vec<f64>,
    expected_im: Vec<f64>,
    relative_l2: f64,
}
fn references() -> Vec<Reference> {
    serde_json::from_str(include_str!("../fixtures/nonlinear-reference.json")).unwrap()
}
fn request() -> Request {
    let mut p = Project {
        globals: "target(x)=x".into(),
        ..Project::default()
    };
    p.fit.function = "target".into();
    p.fit.method = Method::Nonlinear;
    p.fit.sk_steps = "8".into();
    p.fit.wf_steps = "8".into();
    p.fit.refinement_tolerance = "1e-8".into();
    Request::new(&p).unwrap()
}
fn data(r: &Reference) -> (Vec<Sample>, Vec<Node>) {
    let samples =
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
            weight: C::new(r.previous_re[i], r.previous_im[i]),
        })
        .collect();
    (samples, nodes)
}
#[test]
fn real_and_complex_refinements_agree_with_the_authors_octave_reference() {
    on_pool(|| {
        let req = request();
        for r in references() {
            let (samples, mut nodes) = data(&r);
            let previous_error = relative_l2(&model(nodes.clone()).unwrap(), &samples);
            let refined = refine(
                &req,
                &samples,
                &nodes,
                true,
                0,
                &AtomicU64::new(0),
                &mut |_| {},
            )
            .unwrap()
            .unwrap();
            apply_weights(&mut nodes, refined.weights);
            let m = model(nodes).unwrap();
            assert!(
                relative_l2(&m, &samples) <= previous_error,
                "a failed refinement must preserve the previous fit on all fixed samples"
            );
            let error = r
                .x
                .iter()
                .enumerate()
                .map(|(i, &x)| {
                    let expected = C::new(r.expected_re[i], r.expected_im[i]);
                    (m.evaluate(C::new(x, 0.)).unwrap() - expected).norm() / (1. + expected.norm())
                })
                .fold(0_f64, f64::max);
            if error > 1e-7 {
                eprintln!(
                    "{} step {}: weights {:?}, L2 {}, reference {}",
                    r.name,
                    r.step,
                    m.nodes.iter().map(|n| n.weight).collect::<Vec<_>>(),
                    relative_l2(&m, &samples),
                    r.relative_l2
                );
            }
            assert!(
                error < 1e-7,
                "{} step {}: disagreement {error:.3e}",
                r.name,
                r.step
            );
            // The reference's WF diagnostics exclude support rows, even
            // when the newest weight is zero and does not interpolate it.
            let residual = samples
                .iter()
                .filter(|s| r.nodes.iter().all(|&z| z != s.x))
                .fold(0_f64, |norm, s| {
                    norm.hypot((m.evaluate(C::new(s.x, 0.)).unwrap() - s.value).norm())
                });
            let norm = residual
                / samples
                    .iter()
                    .fold(0_f64, |norm, s| norm.hypot(s.value.norm()));
            assert!(
                (norm - r.relative_l2).abs() < 1e-7 * (1. + r.relative_l2),
                "{} step {}: L2 {norm} vs {}",
                r.name,
                r.step,
                r.relative_l2
            );
        }
    });
}
#[test]
fn pseudoinverse_handles_rank_deficiency_and_wide_complex_systems() {
    on_pool(|| {
        for (rows, cols) in [(8, 3), (2, 5), (4, 4)] {
            for complex in [false, true] {
                let a = Mat::from_fn(rows, cols, |i, j| {
                    C::new(
                        (i + j + 1) as f64,
                        if complex { (i + 2 * j) as f64 } else { 0. },
                    )
                });
                let truth: Vec<_> = (0..cols).map(|i| C::new(i as f64 / 7., 0.)).collect();
                let b: Vec<_> = (0..rows)
                    .map(|i| (0..cols).map(|j| a[(i, j)] * truth[j]).sum::<C>())
                    .collect();
                let x = least_squares(&a, &b).unwrap();
                for i in 0..rows {
                    let residual = (0..cols).map(|j| a[(i, j)] * x[j]).sum::<C>() - b[i];
                    assert!(residual.norm() < 1e-11);
                }
            }
        }
    });
}
#[test]
fn cancellation_during_every_refinement_phase_discards_the_run() {
    on_pool(|| {
        let r = references().remove(1);
        let (samples, nodes) = data(&r);
        let req = request();
        for phase in [
            RefinementStage::Sk,
            RefinementStage::WhitfieldProbe,
            RefinementStage::Whitfield,
        ] {
            let latest = AtomicU64::new(0);
            let mut seen = false;
            let result = refine(&req, &samples, &nodes, true, 0, &latest, &mut |p| {
                if let Progress::Refining { stage, .. } = p
                    && std::mem::discriminant(&stage) == std::mem::discriminant(&phase)
                {
                    seen = true;
                    latest.store(1, Ordering::Relaxed);
                }
            })
            .unwrap();
            assert!(seen);
            assert!(result.is_none());
        }
    });
}
