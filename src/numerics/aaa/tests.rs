use super::*;
fn request(body: &str) -> Request {
    let mut p = Project {
        globals: format!("a=2\ntarget(x)={body}").into(),
        ..crate::model::reference_project(crate::model::Mode::Real)
    };
    p.fit.function = "target".into();
    Request::new(&p).expect("request")
}
#[test]
fn automatic_function_fit_resolves_exp_and_a_rational_on_independent_points() {
    for body in ["exp(x)", "1/(x-2)"] {
        let req = request(body);
        let fit = fit(&req, 0, &AtomicU64::new(0), |_| {})
            .expect("fit")
            .expect("current");
        assert!(fit.converged, "{body}: {} {}", fit.error, fit.note);
        let model = model(fit.nodes).expect("fitted model");
        for i in 0..997 {
            let x = -1. + 2. * (i as f64 + 0.271) / 997.;
            let expected = req.value(x).expect("function");
            assert!(
                (model.evaluate(C::new(x, 0.)).expect("finite") - expected).norm() < 1e-11,
                "{body} at {x}"
            );
        }
    }
}
#[test]
fn local_sampling_reuses_unchanged_gaps_and_cached_function_values() {
    let req = request("exp(x)");
    let node = |x| Node {
        row: 0,
        z: C::new(x, 0.),
        value: C::new(0., 0.),
        weight: C::new(1., 0.),
    };
    let before = gap_positions(&[node(-1.), node(0.), node(1.)], [-1., 1.], 3);
    let after = gap_positions(&[node(-1.), node(-0.5), node(0.), node(1.)], [-1., 1.], 3);
    assert_eq!(before.len(), 9);
    assert_eq!(after.len(), 13);
    assert!(
        before
            .iter()
            .filter(|&&x| x >= 0.)
            .all(|x| after.contains(x))
    );
    let new: Vec<_> = after
        .iter()
        .filter(|x| !before.contains(x))
        .copied()
        .collect();
    assert_eq!(new.len(), 4);
    assert!(new.iter().all(|&x| x > -1. && x < 0.));
    let latest = AtomicU64::new(0);
    let mut evaluator = Evaluator::new(&req, 0, &latest);
    on_pool(|| {
        let _ = evaluator
            .samples(&before, false, &mut |_| {})
            .expect("first samples");
        let first = evaluator.cache.len();
        let _ = evaluator
            .samples(&after, false, &mut |_| {})
            .expect("refined samples");
        assert_eq!(evaluator.cache.len(), first + new.len());
        let _ = evaluator
            .samples(&after, false, &mut |_| {})
            .expect("cached samples");
        assert_eq!(evaluator.cache.len(), first + new.len());
    });
}
#[test]
fn one_continuum_run_stops_at_its_budget_without_restarting_support_selection() {
    let mut req = request("abs(x)");
    req.max_nodes = 12;
    req.tolerance = 1e-16;
    req.cleanup = false;
    let result = fit(&req, 0, &AtomicU64::new(0), |_| {})
        .expect("fit")
        .expect("current");
    assert_eq!(result.nodes.len(), 12);
    assert!(!result.converged);
    assert!(result.note.contains("budget reached"));
    assert_eq!(
        result.history.iter().map(|s| s.nodes).collect::<Vec<_>>(),
        (2..=12).collect::<Vec<_>>()
    );
    assert!(result.history.iter().all(|s| s.grid <= 70));
    assert!(result.error.is_finite());
    let req = request("exp(x)");
    let result = fit(&req, 0, &AtomicU64::new(0), |_| {})
        .expect("smooth fit")
        .expect("current");
    assert!(result.converged);
    assert!(result.grid < 100);
    assert!(
        result.evaluations < 600,
        "{} evaluations",
        result.evaluations
    );
}
#[test]
fn fixed_degree_runs_to_the_requested_support_count_and_validates_refined_grids() {
    let mut req = request("exp(x)");
    req.fixed_degree = true;
    req.max_nodes = 4;
    req.lawson = 5;
    let fit = fit(&req, 0, &AtomicU64::new(0), |_| {})
        .expect("fit")
        .expect("current");
    assert_eq!(fit.nodes.len(), 4);
    assert!(!fit.converged);
    assert_eq!(fit.history.last().expect("last iteration").nodes, 4);
    assert!(fit.history.iter().all(|s| s.grid < 100));
    assert!(fit.relative_error < 1e-6, "{}", fit.relative_error);
    assert!(fit.error > 0. && fit.elapsed_ms > 0.);
}
#[test]
fn complex_functions_sign_option_zero_function_and_cancellation() {
    let mut req = request("exp(i*x)");
    req.mode = Mode::Complex;
    req.sign = true;
    let result = fit(&req, 0, &AtomicU64::new(0), |_| {})
        .expect("complex")
        .expect("current");
    assert!(result.relative_error < 1e-10, "{}", result.error);
    let req = request("0");
    let result = fit(&req, 0, &AtomicU64::new(0), |_| {})
        .expect("zero")
        .expect("current");
    assert!(result.converged);
    assert!(result.nodes.len() <= 2);
    assert!(
        fit(&req, 0, &AtomicU64::new(1), |_| {})
            .expect("cancelled")
            .is_none()
    );
}

#[test]
fn right_only_svd_preserves_singular_values_and_the_full_nullspace() {
    on_pool(|| {
        for (rows, cols) in [(257, 7), (7, 7), (3, 7)] {
            for complex in [false, true] {
                let a = Mat::from_fn(rows, cols, |i, j| {
                    C::new(
                        ((i * 17 + j * 31 + 1) as f64).sin(),
                        if complex {
                            ((i * 13 + j * 7 + 2) as f64).cos()
                        } else {
                            0.
                        },
                    )
                });
                let (s, v) = right_svd(&a).expect("right-only SVD");
                let reference = a.singular_values().expect("singular values");
                for i in 0..rows.min(cols) {
                    assert!((s[i].re - reference[i]).abs() < 1e-11);
                }
                for j in 0..cols {
                    let norm = (0..rows)
                        .map(|i| {
                            (0..cols)
                                .map(|k| a[(i, k)] * v[(k, j)])
                                .sum::<C>()
                                .norm_sqr()
                        })
                        .sum::<f64>()
                        .sqrt();
                    let expected = if j < rows { s[j].re } else { 0. };
                    assert!(
                        (norm - expected).abs() < 1e-11,
                        "{rows}x{cols} column {j}: {norm} != {expected}"
                    );
                    for k in 0..cols {
                        let dot = (0..cols).map(|i| v[(i, j)].conj() * v[(i, k)]).sum::<C>();
                        assert!((dot - C::new(if j == k { 1. } else { 0. }, 0.)).norm() < 1e-11);
                    }
                }
            }
        }
    });
}

#[test]
fn sampling_cancels_mid_grid_and_a_new_fit_can_complete() {
    let req = request("exp(x)");
    let latest = AtomicU64::new(0);
    let positions: Vec<_> = (0..16385).map(|i| -1. + 2. * i as f64 / 16384.).collect();
    let mut evaluator = Evaluator::new(&req, 0, &latest);
    let mut completed = 0;
    let result = on_pool(|| {
        evaluator.samples(&positions, false, &mut |p| {
            if let Progress::Sampling { completed: n, .. } = p {
                completed = n;
                if n >= 512 {
                    latest.store(1, Ordering::Relaxed);
                }
            }
        })
    })
    .expect("cancelled sampling");
    assert!(result.is_none());
    assert_eq!(completed, 512);
    assert_eq!(evaluator.cache.len(), 512);
    let mut phases = Vec::new();
    let result = fit(&req, 1, &latest, |p| phases.push(p.phase()))
        .expect("new fit")
        .expect("current");
    assert!(result.converged);
    for phase in [0, 1, 2, 4, 5] {
        assert!(phases.contains(&phase), "missing phase {phase}");
    }
}

#[test]
fn cancellation_during_solve_cleanup_and_lawson_discards_the_fit() {
    for phase in [1, 2, 3, 4, 5] {
        let mut req = request("exp(x)");
        if phase == 3 {
            req.lawson = 3;
        }
        let latest = AtomicU64::new(0);
        let mut reached = false;
        let result = fit(&req, 0, &latest, |p| {
            if p.phase() == phase {
                reached = true;
                latest.store(1, Ordering::Relaxed);
            }
        })
        .expect("cancelled fit");
        assert!(reached, "missing phase {phase}");
        assert!(result.is_none(), "obsolete fit at phase {phase}");
    }
}

#[test]
#[ignore = "AAA timing harness; run with --release --ignored --nocapture"]
fn benchmark_aaa() {
    let serial = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .expect("serial pool");
    let workers = on_pool(rayon::current_num_threads);
    for (body, fixed, nodes, lawson) in [
        ("abs(x)", false, 40, 0),
        ("tanh(30*x)", true, 24, 4),
        ("exp(i*x)", true, 12, 4),
    ] {
        let mut req = request(body);
        req.mode = Mode::Complex;
        req.fixed_degree = fixed;
        req.max_nodes = nodes;
        req.lawson = lawson;
        for parallel in [false, true] {
            let start = std::time::Instant::now();
            let result = if parallel {
                fit(&req, 0, &AtomicU64::new(0), |_| {})
            } else {
                serial.install(|| fit_impl(&req, 0, &AtomicU64::new(0), |_| {}))
            }
            .expect("fit")
            .expect("current");
            eprintln!(
                "{body}, fixed={fixed}, Lawson={lawson}, {} threads: {:.1} ms · {} nodes · relative error {:.3e}",
                if parallel { workers } else { 1 },
                start.elapsed().as_secs_f64() * 1000.,
                result.nodes.len(),
                result.relative_error
            );
            // A cusp can exhaust the finite sample/support budget; this is a
            // timing workload, with accuracy checked by the independent-point tests.
            assert!(result.relative_error.is_finite());
        }
    }
}
