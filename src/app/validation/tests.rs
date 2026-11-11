use super::*;
#[test]
fn independent_complex_component_fields_survive_round_trip_through_real_mode() {
    let mut app = super::super::tests::app(Mode::Complex);
    app.busy = true;
    let _ = app.update(Message::Edit(0, 1, "2*a".into()));
    let _ = app.update(Message::EditImaginary(0, 1, "sin(pi/2)*4".into()));
    assert_eq!(
        app.compiled.as_ref().expect("complex").nodes[0].value,
        C::new(3., 4.)
    );
    let _ = app.update(Message::Mode(Mode::Real));
    assert!(app.error.is_none());
    assert_eq!(app.points.len(), 3);
    assert_eq!(
        app.compiled.as_ref().expect("real").nodes[0].value,
        C::new(3., 0.)
    );
    let _ = app.update(Message::Mode(Mode::Complex));
    assert_eq!(
        app.compiled.as_ref().expect("restored complex").nodes[0].value,
        C::new(3., 4.)
    );
}
#[test]
fn invalid_parameter_keeps_points_and_other_parameters_can_still_move() {
    let mut app = super::super::tests::app(Mode::Complex);
    app.busy = true;
    let old = app.points[0].value;
    let _ = app.update(Message::Edit(0, 1, "sin(".into()));
    assert!(app.error.is_some());
    assert!(app.compiled.is_none());
    assert_eq!(app.points.len(), 3);
    assert_eq!(app.points[0].value, old);
    assert!(app.issues.contains_key(&InputKey::Parameter(0, 1, 0)));
    let _ = app.update(Message::EditImaginary(0, 1, "5".into()));
    assert_eq!(app.points[0].value, C::new(old.re, 5.));
    let _ = app.update(Message::Plot(Interaction::Move(
        Target::Parameter(2, 1),
        C::new(7., 3.),
    )));
    assert_eq!(app.points[2].value, C::new(7., 3.));
    assert!(app.error.is_some());
    let _ = app.update(Message::Edit(0, 1, "2".into()));
    assert!(app.error.is_none());
    assert!(app.issues.is_empty());
}
#[test]
fn invalid_globals_and_settings_retain_handles_and_mark_the_responsible_box() {
    let mut app = super::super::tests::app(Mode::Real);
    let _ = app.update(Message::Field(Field::Samples, "oops".into()));
    assert_eq!(app.points.len(), 3);
    assert!(
        app.issues
            .contains_key(&InputKey::PathSetting(0, Field::Samples))
    );
    let _ = app.update(Message::Field(Field::Samples, "1600".into()));
    app.project.globals = "a =".into();
    app.rebuild();
    assert_eq!(app.points.len(), 3);
    assert!(app.issues.contains_key(&InputKey::GlobalExpression(0)));
    app.project.globals = String::new().into();
    app.rebuild();
    assert!(
        app.issues.contains_key(&InputKey::Parameter(0, 0, 0)),
        "removed globals must not silently resolve through a stale variable"
    );
}
#[test]
fn projected_duplicate_nodes_are_visible_and_can_be_dragged_apart() {
    let mut app = super::super::tests::app(Mode::Complex);
    app.busy = true;
    let _ = app.update(Message::Plot(Interaction::Move(
        Target::Parameter(0, 0),
        C::new(0., 2.),
    )));
    let _ = app.update(Message::Mode(Mode::Real));
    assert!(app.error.is_some());
    assert_eq!(app.points.len(), 3);
    assert!(app.issues.contains_key(&InputKey::Parameter(0, 0, 0)));
    let _ = app.update(Message::Plot(Interaction::Move(
        Target::Parameter(0, 0),
        C::new(-2., 0.),
    )));
    assert!(app.error.is_none());
    assert_eq!(app.points.len(), 3);
}
