use super::*;
pub(super) fn app(mode: Mode) -> App {
    // Construct without scheduling a worker; numerical state is enough here.
    let mut app = App {
        project: model::reference_project(mode),
        numeric_editors: Default::default(),
        panes: pane_grid::State::new(Pane::Editor).0,
        tab: Tab::Nodes,
        compiled: None,
        points: Vec::new(),
        issues: std::collections::BTreeMap::new(),
        plan: None,
        data: None,
        error: None,
        axis: axis_strings(Bounds::default()),
        revision: 0,
        visual_revision: 0,
        latest: Arc::new(AtomicU64::new(0)),
        busy: false,
        dirty: false,
        image_revision: 0,
        image_allocation: None,
        domain_key: None,
        line_key: None,
        domain_revision: 0,
        domain_pending: false,
        line_pending: false,
        draw_path: false,
        path: String::new(),
        notice: String::new(),
        appearance_ui: Default::default(),
        history: Default::default(),
        hovered_entry: None,
        last_hovered_entry: None,
        front_entry: None,
        entry_drag: None,
        fit: FitState::default(),
        spectrum: SpectrumState::default(),
        control_drafts: Vec::new(),
        active_path: 0,
        collapsed_paths: Default::default(),
        plot_size: None,
    };
    app.project.settings.resolve_path_kinds();
    app.control_drafts = control_strings(&app.project);
    app.rebuild();
    app
}
#[test]
fn real_drag_changes_only_selected_scalar_and_preserves_free_y() {
    let mut app = app(Mode::Real);
    let _ = app.update(Message::Lock(0, 0, 0, true));
    let _ = app.update(Message::Lock(0, 1, 0, true));
    let _ = app.update(Message::Plot(Interaction::Move(
        Target::Parameter(0, 1),
        C::new(2.3, -1.7),
    )));
    let n = &app.compiled.as_ref().expect("valid drag").nodes[0];
    assert_eq!(n.z, C::new(-2., 0.));
    assert_eq!(n.value, C::new(2., 0.));
    assert_eq!(app.project.entries[0].real_y[1], -1.7);
    let _ = app.update(Message::Plot(Interaction::Move(
        Target::Parameter(0, 2),
        C::new(3.6, 1.25),
    )));
    let n = &app.compiled.as_ref().expect("weight drag").nodes[0];
    assert_eq!(n.z, C::new(-2., 0.));
    assert_eq!(n.value, C::new(2., 0.));
    assert_eq!(n.weight, C::new(3.6, 0.));
    assert_eq!(app.project.entries[0].real_y[2], 1.25);
    assert!(
        app.data
            .as_ref()
            .is_some_and(|data| data.samples.len() == 1600)
    ); // output is ready in this update
}
#[test]
fn complex_drag_snaps_only_locked_component() {
    let mut app = app(Mode::Complex);
    let _ = app.update(Message::Lock(0, 2, 1, true));
    let _ = app.update(Message::Plot(Interaction::Move(
        Target::Parameter(0, 2),
        C::new(0.35, 1.7),
    )));
    assert_eq!(
        app.compiled.as_ref().expect("valid drag").nodes[0].weight,
        C::new(0.35, 2.)
    );
}
#[test]
fn invalid_axis_stays_invalid_across_other_edits_and_recovers() {
    let mut app = app(Mode::Real);
    let _ = app.update(Message::Axis(0, "bad".into()));
    let _ = app.update(Message::Edit(0, 1, "3".into()));
    assert!(app.error.is_some());
    assert!(!app.dirty);
    let _ = app.update(Message::Axis(0, "-4".into()));
    assert!(app.error.is_none());
    assert!(app.data.is_some());
}
#[test]
fn stale_render_cannot_replace_current_output() {
    let mut app = app(Mode::Real);
    let stale = app.revision;
    let _ = app.update(Message::Edit(0, 1, "2".into()));
    let current = Arc::clone(&app.data.as_ref().expect("inline curve").samples);
    let _ = app.update(Message::Rendered(RenderResult {
        domain_revision: app.domain_revision,
        epoch: app.latest.load(Ordering::Relaxed),
        revision: stale,
        complete: false,
        data: Some(Arc::new(RenderData {
            samples: Arc::from([]),
            image: None,
            domain: Bounds::default(),
            elapsed_ms: 0.,
            missing: 0,
        })),
    }));
    assert!(Arc::ptr_eq(
        &current,
        &app.data.as_ref().expect("current curve").samples
    ));
}
#[test]
fn large_sample_counts_hold_the_complete_curve_until_the_full_replacement() {
    let mut app = app(Mode::Real);
    app.busy = true;
    let original = Arc::clone(&app.data.as_ref().expect("complete curve").samples);
    let _ = app.update(Message::Field(Field::Samples, "100000".into()));
    assert!(Arc::ptr_eq(
        &original,
        &app.data.as_ref().expect("retained curve").samples
    ));
    assert!(app.line_pending);
    let epoch = app.latest.load(Ordering::Relaxed);
    let full = sampling::generate(
        app.compiled.as_ref().expect("model"),
        &app.project.settings,
        app.plan.as_ref().expect("plan"),
        epoch,
        &app.latest,
        |_| {},
    )
    .expect("full replacement");
    app.dirty = false;
    let _ = app.update(Message::Rendered(RenderResult {
        domain_revision: app.domain_revision,
        epoch,
        revision: app.revision,
        complete: true,
        data: Some(Arc::new(full)),
    }));
    assert_eq!(app.data.as_ref().expect("full curve").samples.len(), 100000);
    assert!(!app.line_pending);
    // Another edit has the same sample count but still needs its own numerical result.
    app.busy = true;
    let previous = Arc::clone(&app.data.as_ref().expect("complete curve").samples);
    let _ = app.update(Message::Edit(0, 1, "7".into()));
    assert!(app.line_pending);
    assert!(Arc::ptr_eq(
        &previous,
        &app.data.as_ref().expect("retained curve").samples
    ));
    let full = sampling::sample_line(
        app.compiled.as_ref().expect("model"),
        &app.project.settings,
        app.plan.as_ref().expect("plan"),
    );
    let _ = app.update(Message::Rendered(RenderResult {
        domain_revision: app.domain_revision,
        epoch,
        revision: app.revision,
        complete: true,
        data: Some(Arc::new(full)),
    }));
    assert!(!app.line_pending);
    assert!(!Arc::ptr_eq(
        &previous,
        &app.data.as_ref().expect("replacement curve").samples
    ));
    assert!(!app.busy);
    assert!(!app.dirty);
}
#[test]
fn continuous_drag_updates_the_curve_even_while_a_colour_job_is_busy() {
    for mode in [Mode::Real, Mode::Complex] {
        let mut app = app(mode);
        app.busy = true; // Hold a worker in flight across the entire drag burst.
        for i in 0..200 {
            let value = C::new(0.25 + i as f64 * 0.001, 0.5);
            let _ = app.update(Message::Plot(Interaction::Move(
                Target::Parameter(0, 1),
                value,
            )));
            let data = app
                .data
                .as_ref()
                .expect("curve ready before returning from update");
            let sample = &data.samples[800];
            let expected = app.compiled.as_ref().expect("model").evaluate(sample.input);
            match (sample.output, expected) {
                (Some(a), Some(b)) => assert!((a - b).norm() <= 1e-11 * (1. + b.norm())),
                (None, None) => {}
                _ => panic!("pole classification differs"),
            }
            assert_eq!(data.samples.len(), 1600);
        }
    }
}
#[test]
fn completed_colour_frames_are_presented_during_motion_without_rolling_back_the_curve() {
    let mut app = app(Mode::Complex);
    app.busy = true;
    let _ = app.update(Message::Edit(0, 1, "2".into()));
    let epoch = app.latest.load(Ordering::Relaxed);
    let old_revision = app.revision;
    let mut rendered = (**app.data.as_ref().expect("data")).clone();
    let image = iced::widget::image::Handle::from_rgba(1, 1, vec![1, 2, 3, 255]);
    rendered.image = Some(image.clone());
    let _ = app.update(Message::Edit(0, 1, "3".into()));
    assert_eq!(
        epoch,
        app.latest.load(Ordering::Relaxed),
        "motion must not cancel each colour job"
    );
    let current_samples = Arc::clone(&app.data.as_ref().expect("data").samples);
    let vector_revision = app.visual_revision;
    app.dirty = false; // Test completion without launching another asynchronous worker.
    let _ = app.update(Message::Rendered(RenderResult {
        domain_revision: app.domain_revision,
        epoch,
        revision: old_revision,
        complete: true,
        data: Some(Arc::new(rendered)),
    }));
    assert!(
        app.data.as_ref().expect("old image").image.is_none(),
        "CPU completion must not expose an unuploaded texture"
    );
    use iced::advanced::image::Renderer as _;
    let renderer = iced_tiny_skia::Renderer::new(Default::default());
    let allocation = renderer.load_image(&image).expect("software allocation");
    let _ = app.update(Message::ImageReady(
        TextureUpload {
            epoch,
            domain_revision: app.domain_revision,
            domain: Bounds::default(),
            elapsed_ms: 1.,
        },
        Ok(allocation),
    ));
    let data = app.data.as_ref().expect("data");
    assert_eq!(data.image.as_ref(), Some(&image));
    assert!(Arc::ptr_eq(&current_samples, &data.samples));
    assert_eq!(
        app.visual_revision, vector_revision,
        "a texture upload must not rebuild vector geometry"
    );
}
#[test]
fn changing_resolution_rejects_images_from_the_previous_setting() {
    let mut app = app(Mode::Complex);
    app.busy = true;
    let _ = app.update(Message::Edit(0, 1, "2".into()));
    let epoch = app.latest.load(Ordering::Relaxed);
    let mut old = (**app.data.as_ref().expect("data")).clone();
    old.image = Some(iced::widget::image::Handle::from_rgba(
        1,
        1,
        vec![1, 2, 3, 255],
    ));
    let _ = app.update(Message::Appearance(appearance::Action::Number(
        appearance::Number::Resolution,
        1536.to_string(),
    )));
    assert!(app.dirty);
    assert_ne!(epoch, app.latest.load(Ordering::Relaxed));
    app.dirty = false;
    let _ = app.update(Message::Rendered(RenderResult {
        domain_revision: app.domain_revision,
        epoch,
        revision: app.revision,
        complete: true,
        data: Some(Arc::new(old)),
    }));
    assert!(app.data.as_ref().expect("data").image.is_none());
}
