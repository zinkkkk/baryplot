use super::*;
use std::{hint::black_box, time::Instant};
#[test]
#[ignore = "timing harness; run with --release --ignored --nocapture"]
fn benchmark_sampling() {
    for mode in [Mode::Real, Mode::Complex] {
        let project = preset(mode);
        let model = project.compile().expect("model");
        let mut settings = project.settings;
        settings.coloring = false;
        let plan = sampling::plan(&settings, &model).expect("plan");
        let generation = Arc::new(AtomicU64::new(1));
        let start = Instant::now();
        for _ in 0..200 {
            for i in 0..1600 {
                black_box(model.evaluate(C::new(-2. + 4. * i as f64 / 1599., 0.)));
            }
        }
        println!(
            "{mode}: scalar math, 1600 samples: {:.3} ms",
            start.elapsed().as_secs_f64() * 1000. / 200.
        );
        let _ = sampling::generate(&model, &settings, &plan, 1, &generation, |_| {});
        let start = Instant::now();
        for _ in 0..100 {
            black_box(sampling::generate(
                &model,
                &settings,
                &plan,
                1,
                &generation,
                |_| {},
            ));
        }
        println!(
            "{mode}: current sampling pipeline: {:.3} ms",
            start.elapsed().as_secs_f64() * 1000. / 100.
        );
        let mut app = tests::app(mode);
        app.busy = true;
        let start = Instant::now();
        for i in 0..1000 {
            let _ = black_box(app.update(Message::Plot(Interaction::Move(
                Target::Parameter(0, 1),
                C::new(0.25 + i as f64 * 0.0001, 0.5),
            ))));
        }
        println!(
            "{mode}: complete drag update + fresh curve: {:.3} ms",
            start.elapsed().as_secs_f64() * 1000. / 1000.
        );
        if mode == Mode::Complex {
            settings.coloring = true;
            let start = Instant::now();
            for _ in 0..10 {
                black_box(sampling::generate(
                    &model,
                    &settings,
                    &plan,
                    1,
                    &generation,
                    |_| {},
                ));
            }
            println!(
                "{mode}: with 640px domain: {:.3} ms",
                start.elapsed().as_secs_f64() * 1000. / 10.
            );
        }
    }
}
#[test]
fn starter_presets_are_self_contained_and_real_demo_is_rational() {
    for mode in [Mode::Real, Mode::Complex] {
        let project = preset(mode);
        assert!(project.globals.is_empty());
        let model = project.compile().expect("self-contained preset");
        let _ = sampling::plan(&project.settings, &model).expect("valid preset settings");
        if mode == Mode::Real {
            for x in [-3., -1., -0.3, 0., 0.5, 1., 3.] {
                let expected = (x + 1.) / (x * x + 1.);
                assert!((model.evaluate(C::new(x, 0.)).unwrap().re - expected).abs() < 1e-12);
            }
        } else {
            assert!(project.entries.iter().all(|e| e.imaginary.is_some()));
            assert!(model.nodes.iter().all(|n| n.z.im != 0.));
        }
    }
}
#[test]
fn editor_collapse_preserves_resized_layout_view_and_function_samples() {
    let (mut app, _) = App::new(preset(Mode::Real));
    let pane_grid::Node::Split { id: split, .. } = app.panes.layout().clone() else {
        panic!("split layout")
    };
    let _ = app.update(Message::Resize(pane_grid::ResizeEvent {
        split,
        ratio: 0.43,
    }));
    let layout = format!("{:?}", app.panes.layout());
    let bounds = app.project.settings.bounds;
    let samples = Arc::clone(&app.data.as_ref().unwrap().samples);
    let revision = app.revision;
    for _ in 0..2 {
        let _ = app.update(Message::ToggleEditor);
        assert!(matches!(
            app.panes.maximized().and_then(|p| app.panes.get(p)),
            Some(Pane::Plot)
        ));
        let _ = app.update(Message::ToggleEditor);
        assert!(app.panes.maximized().is_none());
        assert_eq!(format!("{:?}", app.panes.layout()), layout);
    }
    let _ = app.update(Message::ToggleEditor);
    // The toolbar's spectrum gear must reveal its editor page when hidden.
    app.spectrum.paused = true;
    let _ = app.update(Message::Tab(Tab::Spectrum));
    assert!(app.panes.maximized().is_none());
    assert_eq!(app.tab, Tab::Spectrum);
    assert_eq!(app.project.settings.bounds, bounds);
    assert_eq!(app.revision, revision);
    assert!(Arc::ptr_eq(&samples, &app.data.as_ref().unwrap().samples));
}
#[test]
fn real_interval_handles_update_samples_without_changing_function_or_colour_domain() {
    let mut app = super::tests::app(Mode::Real);
    app.project.settings.real_coloring = true;
    app.rebuild();
    app.busy = true;
    let entries = serde_json::to_string(&app.project.entries).unwrap();
    let domain = app.domain_revision;
    let bounds = app.project.settings.bounds;
    let samples = Arc::clone(&app.data.as_ref().unwrap().samples);
    let _ = app.update(Message::Plot(Interaction::Move(
        Target::Interval(0),
        C::new(-1.5, 99.),
    )));
    assert_eq!(app.plan.as_ref().unwrap().paths[0].interval, [-1.5, 2.]);
    assert_eq!(app.project.settings.input.interval[0], "-1.5");
    assert_eq!(app.domain_revision, domain);
    assert_eq!(app.project.settings.bounds, bounds);
    assert_eq!(
        serde_json::to_string(&app.project.entries).unwrap(),
        entries
    );
    assert!(!Arc::ptr_eq(&samples, &app.data.as_ref().unwrap().samples));
    let _ = app.update(Message::Plot(Interaction::Move(
        Target::Interval(0),
        C::new(3., 0.),
    )));
    let p = &app.plan.as_ref().unwrap().paths[0];
    assert!(p.interval[0] < p.interval[1]);
    assert!(app.error.is_none());
    let _ = app.update(Message::Field(Field::Start, "1e-12".into()));
    let _ = app.update(Message::Field(Field::End, "2e-12".into()));
    let _ = app.update(Message::Plot(Interaction::Move(
        Target::Interval(0),
        C::new(3e-12, 0.),
    )));
    let p = &app.plan.as_ref().unwrap().paths[0];
    assert!(p.interval[0] > 0. && p.interval[0] < 2e-12);
    let _ = app.update(Message::Plot(Interaction::Move(
        Target::Interval(1),
        C::new(0., 0.),
    )));
    assert!(app.error.is_none());
    let p = &app.plan.as_ref().unwrap().paths[0];
    assert!(p.interval[1] > p.interval[0]);
}
