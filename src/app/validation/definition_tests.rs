use super::*;
#[test]
fn new_data_rows_avoid_the_global_namespace() {
    let mut app = super::super::tests::app(Mode::Real);
    let _ = app.update(Message::AddGlobal);
    let _ = app.update(Message::GlobalEdit(1, GlobalField::Name, "p4".into()));
    let _ = app.update(Message::Add);
    assert_eq!(app.project.entries[3].name, "p5");
    assert!(app.error.is_none());
}
#[test]
fn table_edits_and_dragging_update_forward_references_and_preserve_invalid_handles() {
    let mut app = super::super::tests::app(Mode::Real);
    app.busy = true;
    let _ = app.update(Message::AddGlobal);
    let _ = app.update(Message::GlobalKind(1, model::DefinitionKind::Function));
    let _ = app.update(Message::GlobalEdit(1, GlobalField::Name, "wave".into()));
    let _ = app.update(Message::GlobalEdit(
        1,
        GlobalField::Expression,
        "a*x".into(),
    ));
    let _ = app.update(Message::Edit(0, 1, "wave(p3[n])".into()));
    assert_eq!(app.points[0].value, C::new(4., 0.));
    let _ = app.update(Message::GlobalEdit(0, GlobalField::Expression, "3".into()));
    assert_eq!(app.points[0].value, C::new(9., 0.));
    let _ = app.update(Message::Plot(Interaction::Move(
        Target::Parameter(2, 0),
        C::new(4.25, 0.),
    )));
    assert_eq!(app.points[0].value, C::new(12.75, 0.));
    let _ = app.update(Message::Lock(2, 0, 0, true));
    assert_eq!(app.points[0].value, C::new(12., 0.));
    let _ = app.update(Message::GlobalEdit(
        1,
        GlobalField::Parameters,
        "x,x".into(),
    ));
    assert!(app.issues.contains_key(&InputKey::GlobalParameters(1)));
    assert_eq!(app.points.len(), 3);
    assert_eq!(app.points[0].value, C::new(12., 0.));
    let _ = app.update(Message::Plot(Interaction::Move(
        Target::Parameter(1, 1),
        C::new(5., 0.),
    )));
    assert_eq!(app.points[1].value, C::new(5., 0.));
    let _ = app.update(Message::GlobalEdit(1, GlobalField::Parameters, "x".into()));
    assert!(app.error.is_none());
    assert!(app.issues.is_empty());
    let _ = app.update(Message::RemoveGlobal(1));
    assert!(app.issues.contains_key(&InputKey::Parameter(0, 1, 0)));
    assert_eq!(app.points.len(), 3);
}
