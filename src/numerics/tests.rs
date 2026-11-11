use super::*;
use crate::expression::Variables;
#[test]
fn numerical_parallelism_uses_a_separate_pool_and_reserves_one_cpu() {
    assert_eq!(worker_count(1), 1);
    assert_eq!(worker_count(2), 1);
    assert_eq!(worker_count(16), 15);
    on_pool(|| {
        assert!(
            std::thread::current()
                .name()
                .expect("worker name")
                .starts_with("baryplot-numerics-")
        );
        assert_eq!(
            faer::get_global_parallelism().degree(),
            rayon::current_num_threads()
        );
        if rayon::current_num_threads() > 1 {
            let barrier = std::sync::Barrier::new(2);
            let work = || {
                barrier.wait();
                std::thread::current().id()
            };
            let (a, b) = rayon::join(work, work);
            assert_ne!(a, b);
        }
    });
}
fn rational(nodes: &[C], p: C, q: C, a: C, b: C) -> Compiled {
    let terms = nodes
        .iter()
        .enumerate()
        .map(|(i, &z)| {
            let product = nodes
                .iter()
                .enumerate()
                .filter(|(j, _)| i != *j)
                .map(|(_, w)| z - w)
                .product::<C>();
            Node {
                row: i,
                z,
                value: a / (z - p) + b / (z - q) + C::new(0.2, 0.),
                weight: (z - p) * (z - q) / product,
            }
        })
        .collect();
    Compiled::from_nodes(terms, Variables::new()).expect("rational")
}
#[test]
fn generalized_pencils_find_known_complex_poles_and_refine_residues() {
    let (p, q) = (C::new(-0.3, 0.7), C::new(0.5, -0.4));
    let (a, b) = (C::new(1., 0.2), C::new(-0.5, 0.1));
    let m = rational(
        &[C::new(-1., 0.), C::new(0., 0.2), C::new(1., 0.)],
        p,
        q,
        a,
        b,
    );
    let out = analyze(
        &m,
        AnalysisOptions {
            poles: true,
            zeros: true,
            residues: true,
        },
        0,
        &AtomicU64::new(0),
    )
    .expect("analysis")
    .expect("current");
    assert_eq!(out.poles.len(), 2, "{:?}", out.poles);
    for (pole, residue) in [(p, a), (q, b)] {
        let (i, &found) = out
            .poles
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (**a - pole).norm().total_cmp(&(**b - pole).norm()))
            .expect("pole");
        assert!((found - pole).norm() < 1e-10, "{found} != {pole}");
        assert!((out.residues[i].expect("simple residue") - residue).norm() < 1e-10);
    }
    for z in out.zeros {
        assert!(m.evaluate(z).expect("zero").norm() < 1e-10);
    }
}
#[test]
fn finite_real_pole_zero_and_polynomial_infinite_eigenvalues() {
    let m = Compiled::from_nodes(
        vec![
            Node {
                row: 0,
                z: C::new(-1., 0.),
                value: C::new(2. / 3., 0.),
                weight: C::new(3., 0.),
            },
            Node {
                row: 1,
                z: C::new(1., 0.),
                value: C::new(0., 0.),
                weight: C::new(-1., 0.),
            },
        ],
        Variables::new(),
    )
    .expect("model");
    let out = analyze(
        &m,
        AnalysisOptions {
            poles: true,
            zeros: true,
            residues: true,
        },
        0,
        &AtomicU64::new(0),
    )
    .expect("analysis")
    .expect("current");
    assert_eq!(out.poles.len(), 1);
    assert_eq!(out.zeros.len(), 1);
    assert!((out.poles[0] - C::new(2., 0.)).norm() < 1e-11);
    assert!((out.zeros[0] - C::new(1., 0.)).norm() < 1e-11);
    assert!((out.residues[0].expect("residue") - C::new(1., 0.)).norm() < 1e-10);
    let p = crate::model::reference_project(crate::model::Mode::Real)
        .compile()
        .expect("polynomial");
    let spectrum = analyze(
        &p,
        AnalysisOptions {
            poles: true,
            zeros: true,
            residues: true,
        },
        0,
        &AtomicU64::new(0),
    )
    .expect("polynomial spectrum")
    .expect("current");
    assert!(
        spectrum.poles.is_empty(),
        "polynomial should have only infinite poles: {:?}",
        spectrum.poles
    );
    assert_eq!(spectrum.zeros.len(), 2);
}
#[test]
fn constant_functions_and_cancelled_analysis_have_no_spurious_markers() {
    let mut p = crate::model::reference_project(crate::model::Mode::Real);
    for n in &mut p.entries {
        n.expressions[1] = "2".into();
    }
    let m = p.compile().expect("constant");
    let out = analyze(
        &m,
        AnalysisOptions {
            poles: true,
            zeros: true,
            residues: true,
        },
        0,
        &AtomicU64::new(0),
    )
    .expect("constant")
    .expect("current");
    assert!(out.poles.is_empty() && out.zeros.is_empty());
    let m = crate::model::reference_project(crate::model::Mode::Real)
        .compile()
        .expect("polynomial");
    assert!(
        analyze(
            &m,
            AnalysisOptions {
                poles: true,
                zeros: true,
                residues: true
            },
            0,
            &AtomicU64::new(1)
        )
        .expect("cancelled")
        .is_none()
    );
}
#[test]
fn nonreal_zeros_are_preserved_for_real_and_complex_input_modes() {
    for mode in [crate::model::Mode::Real, crate::model::Mode::Complex] {
        let mut p = crate::model::reference_project(crate::model::Mode::Real);
        p.settings.mode = mode;
        for (entry, value) in p.entries.iter_mut().zip(["5", "1", "5"]) {
            entry.expressions[1] = value.into();
        }
        let model = p.compile().expect("z squared plus one");
        let spectrum = analyze(
            &model,
            AnalysisOptions {
                poles: true,
                zeros: true,
                residues: true,
            },
            0,
            &AtomicU64::new(0),
        )
        .expect("spectrum")
        .expect("current");
        assert_eq!(spectrum.zeros.len(), 2);
        assert!(spectrum.poles.is_empty());
        for expected in [C::i(), -C::i()] {
            assert!(
                spectrum
                    .zeros
                    .iter()
                    .any(|z| (*z - expected).norm() < 1e-11),
                "{mode:?}: {:?}",
                spectrum.zeros
            );
        }
    }
}
