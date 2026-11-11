use super::*;
use crate::model::{Entry, Mode, number};
fn check(model: &Compiled, inputs: &[C]) {
    let mut output = vec![None; inputs.len()];
    model.evaluate_many(inputs, &mut output);
    for (&z, actual) in inputs.iter().zip(output) {
        match (actual, model.evaluate(z)) {
            (Some(a), Some(b)) => assert!(
                (a - b).norm() <= 2e-11 * (1. + b.norm()),
                "z={z} actual={a} expected={b}"
            ),
            (None, None) => {}
            (a, b) => panic!("classification z={z}: {a:?} vs {b:?}"),
        }
    }
}
#[test]
fn general_batches_match_scalar_for_real_and_complex_models_and_tails() {
    for count in [3, 8, 31, 64] {
        for mode in [Mode::Real, Mode::Complex] {
            let mut project = crate::model::reference_project(crate::model::Mode::Real);
            project.settings.mode = mode;
            project.entries = (0..count)
                .map(|i| {
                    let mut e = Entry::new(i);
                    let t = i as f64;
                    e.expressions = [
                        number(C::new(
                            t * 0.13 - 2.,
                            if mode == Mode::Complex {
                                (t * 0.7).sin()
                            } else {
                                0.
                            },
                        )),
                        number(C::new(
                            (t * 0.4).sin(),
                            if mode == Mode::Complex {
                                (t * 0.2).cos()
                            } else {
                                0.
                            },
                        )),
                        number(C::new(
                            if i % 2 == 0 { 1. } else { -0.7 },
                            if mode == Mode::Complex { 0.3 } else { 0. },
                        )),
                    ];
                    e
                })
                .collect();
            let model = project.compile().expect("model");
            let mut inputs: Vec<_> = (0..2053)
                .map(|i| {
                    C::new(
                        i as f64 * 0.006 - 4.,
                        if mode == Mode::Complex {
                            (i as f64 * 0.071).cos() * 2.
                        } else {
                            0.
                        },
                    )
                })
                .collect();
            for n in &model.nodes {
                inputs.push(n.z);
                inputs.push(n.z + C::new(1e-13, -1e-13));
            }
            check(&model, &inputs);
        }
    }
}
#[test]
fn poles_zero_weights_and_extreme_lanes_keep_reference_behavior() {
    let mut project = crate::model::reference_project(crate::model::Mode::Real);
    project.entries.truncate(2);
    project.entries[0].expressions = ["-1".into(), "-1".into(), "1".into()];
    project.entries[1].expressions = ["1".into(), "1".into(), "1".into()];
    let mut inputs = vec![
        C::new(0., 0.),
        C::new(1e-16, 0.),
        C::new(1e-8, 0.),
        C::new(-1., 0.),
        C::new(1., 0.),
    ];
    for v in [1e-300, 1e-160, 1e-100, 1e100, 1e160, 1e300] {
        inputs.push(C::new(v, v));
        inputs.push(C::new(-v, 0.));
    }
    check(&project.compile().expect("pole model"), &inputs);
    project.entries[0].expressions[2] = "0".into();
    check(&project.compile().expect("zero weight"), &inputs);
}
#[test]
fn row_evaluator_matches_batch_inputs() {
    let model = crate::model::reference_project(crate::model::Mode::Real)
        .compile()
        .expect("model");
    let mut output = vec![None; 137];
    model.evaluate_row(-3., 0.05, 0.37, &mut output);
    let inputs: Vec<_> = (0..137)
        .map(|i| C::new(-3. + (i as f64 + 0.5) * 0.05, 0.37))
        .collect();
    check(&model, &inputs);
    for (&z, actual) in inputs.iter().zip(output) {
        let expected = model.evaluate(z).expect("finite");
        assert!((actual.expect("batch") - expected).norm() < 1e-11 * (1. + expected.norm()));
    }
}
#[test]
#[ignore = "release SIMD comparison"]
fn benchmark_batches() {
    use std::{hint::black_box, time::Instant};
    for count in [3, 8, 32] {
        let mut p = crate::model::reference_project(crate::model::Mode::Real);
        p.settings.mode = Mode::Complex;
        p.entries = (0..count)
            .map(|i| {
                let mut e = Entry::new(i);
                e.expressions = [
                    number(C::new(i as f64 * 0.1, (i as f64).sin())),
                    number(C::new((i as f64).cos(), 0.5)),
                    "1".into(),
                ];
                e
            })
            .collect();
        let model = p.compile().expect("model");
        let inputs: Vec<_> = (0..8192)
            .map(|i| C::new(i as f64 * 0.001 - 4., 0.37))
            .collect();
        let mut output = vec![None; inputs.len()];
        let start = Instant::now();
        for _ in 0..100 {
            for (z, out) in inputs.iter().zip(&mut output) {
                *out = model.evaluate(black_box(*z));
            }
            black_box(&output);
        }
        let scalar = start.elapsed().as_secs_f64() * 10.;
        let start = Instant::now();
        for _ in 0..100 {
            model.evaluate_many(black_box(&inputs), &mut output);
            black_box(&output);
        }
        let simd = start.elapsed().as_secs_f64() * 10.;
        println!(
            "{count} complex nodes, 8192 points: scalar {scalar:.3} ms, SIMD {simd:.3} ms ({:.2}x), {:?}",
            scalar / simd,
            level()
        );
    }
}
