use super::*;
#[test]
fn multiple_paths_sample_independently_and_errors_do_not_block_other_paths() {
    let mut app = super::super::tests::app(Mode::Complex);
    app.busy = true;
    let _ = app.update(Message::AddPath(InputKind::Interval));
    let _ = app.update(Message::Field(Field::Start, "-1".into()));
    let _ = app.update(Message::Field(Field::End, "1".into()));
    let _ = app.update(Message::AddPath(InputKind::Point));
    let _ = app.update(Message::ControlAxis(0, 0, "pi/4".into()));
    let _ = app.update(Message::ControlAxis(0, 1, "0.5".into()));
    let data = app.data.as_ref().unwrap();
    assert_eq!(data.samples.len(), 3201);
    let point = &data.samples[3200];
    assert_eq!(point.path, 2);
    assert_eq!(point.input, C::new(std::f64::consts::FRAC_PI_4, 0.5));
    for v in data.samples.iter().filter(|v| v.path == 1) {
        assert_eq!(v.input.im, 0.);
    }
    let domain = app.domain_revision;
    let _ = app.update(Message::ControlAxis(0, 0, "oops(".into()));
    assert!(app.error.is_none());
    assert!(app.issues.contains_key(&InputKey::Control(2, 0, 0)));
    assert_eq!(app.plan.as_ref().unwrap().path_errors.len(), 1);
    assert_eq!(app.data.as_ref().unwrap().samples.len(), 3200);
    assert_eq!(app.domain_revision, domain);
    let _ = app.update(Message::PathEnabled(false));
    assert!(app.plan.as_ref().unwrap().path_errors.is_empty());
    let _ = app.update(Message::SelectPath(1));
    assert!(app.issues.is_empty());
    let _ = app.update(Message::Plot(Interaction::Move(
        Target::PathInterval(1, 0),
        C::new(-1.5, 2.),
    )));
    assert_eq!(app.input().interval[0], "-1.5");
    let _ = app.update(Message::RemovePath);
    assert_eq!(app.project.settings.additional_paths.len(), 1);
    assert_eq!(app.active_path, 1);
    assert_eq!(app.input().kind(Mode::Complex), InputKind::Point);
}
#[test]
fn mode_switch_preserves_every_path_and_hidden_imaginary_coordinates() {
    let mut app = super::super::tests::app(Mode::Complex);
    app.busy = true;
    let _ = app.update(Message::AddPath(InputKind::Interval));
    let _ = app.update(Message::AddPath(InputKind::Point));
    let _ = app.update(Message::ControlAxis(0, 1, "undefined(".into()));
    let before = (
        app.project.settings.input.clone(),
        app.project.settings.additional_paths.clone(),
    );
    let _ = app.update(Message::Mode(Mode::Real));
    assert!(
        app.plan.as_ref().unwrap().path_errors.is_empty(),
        "hidden imaginary drafts do not break real evaluation"
    );
    assert_eq!(
        app.project.settings.input.kind(Mode::Real),
        InputKind::Interval
    );
    assert_eq!(
        app.plan.as_ref().unwrap().paths[0].kind,
        InputKind::Interval
    );
    // Saving/reopening in real mode must retain the hidden complex curve.
    let saved = serde_json::to_string(&app.project).unwrap();
    let mut loaded: Project = serde_json::from_str(&saved).unwrap();
    loaded.settings.resolve_path_kinds();
    assert_eq!(loaded.settings.input, before.0);
    assert_eq!(app.project.settings.input, before.0);
    assert_eq!(app.project.settings.additional_paths, before.1);
    assert!(
        app.data
            .as_ref()
            .unwrap()
            .samples
            .iter()
            .all(|s| s.input.im == 0.)
    );
    let _ = app.update(Message::Mode(Mode::Complex));
    assert!(app.plan.as_ref().unwrap().path_errors.contains_key(&2));
    assert_eq!(app.project.settings.input, before.0);
    assert_eq!(app.project.settings.additional_paths, before.1);
}
#[test]
fn legacy_flat_settings_and_multiple_paths_round_trip() {
    let mut app = super::super::tests::app(Mode::Complex);
    app.busy = true;
    let legacy = serde_json::json!({"mode":"Complex","interval":["-7","8"],"samples":"256","control_points":[[1,2],[3,4]],"curve":"Line"});
    let mut settings: model::Settings = serde_json::from_value(legacy).unwrap();
    settings.resolve_path_kinds();
    assert_eq!(settings.input.interval, ["-7", "8"]);
    assert_eq!(settings.input.control_points, [[1., 2.], [3., 4.]]);
    assert_eq!(settings.input.kind(Mode::Complex), InputKind::Curve);
    app.project.settings = settings;
    let _ = app.update(Message::AddPath(InputKind::Interval));
    let _ = app.update(Message::AddPath(InputKind::Point));
    let _ = app.update(Message::ControlAxis(0, 0, "pi/3".into()));
    let text = serde_json::to_string(&app.project).unwrap();
    let restored: Project = serde_json::from_str(&text).unwrap();
    assert_eq!(restored, app.project);
    let bytes = rmp_serde::to_vec_named(&app.project).unwrap();
    let restored: Project = rmp_serde::from_slice(&bytes).unwrap();
    assert_eq!(restored, app.project);
}
