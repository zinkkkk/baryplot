use super::*;
fn send(app: &mut App, path: usize, message: Message) {
    let _ = app.update(Message::EditPath(path, Box::new(message)));
}
#[test]
fn cards_collapse_independently_without_resampling_and_edits_keep_their_target() {
    let mut app = super::super::tests::app(Mode::Complex);
    app.busy = true;
    let _ = app.update(Message::AddPath(InputKind::Interval));
    let _ = app.update(Message::AddPath(InputKind::Point));
    let project = app.project.clone();
    let revision = app.revision;
    let domain = app.domain_revision;
    let samples = app.data.as_ref().unwrap().samples.clone();
    for path in [0, 2] {
        let _ = app.update(Message::TogglePath(path));
    }
    assert_eq!(
        app.collapsed_paths.iter().copied().collect::<Vec<_>>(),
        vec![0, 2]
    );
    let _ = app.update(Message::TogglePath(0));
    assert_eq!(app.active_path, 0);
    assert!(app.collapsed_paths.contains(&2));
    assert!(!app.collapsed_paths.contains(&0));
    assert!(!app.collapsed_paths.contains(&1));
    assert_eq!(app.project, project);
    assert_eq!(app.revision, revision);
    assert_eq!(app.domain_revision, domain);
    assert!(Arc::ptr_eq(&samples, &app.data.as_ref().unwrap().samples));
    send(&mut app, 1, Message::Field(Field::Start, "-1.25".into()));
    send(&mut app, 2, Message::ControlAxis(0, 1, "0.75".into()));
    send(&mut app, 0, Message::PathName("Arc".into()));
    assert_eq!(app.project.settings.path(1).unwrap().interval[0], "-1.25");
    assert_eq!(app.project.settings.path(2).unwrap().drafts()[0][1], "0.75");
    assert_eq!(app.project.settings.input.name, "Arc");
    assert_eq!(
        app.project.settings.input.control_points,
        project.settings.input.control_points
    );
    assert!(
        app.collapsed_paths.contains(&2),
        "editing a collapsed header does not expand the card"
    );
    send(&mut app, 0, Message::ControlAxis(0, 0, "broken(".into()));
    send(
        &mut app,
        2,
        Message::ControlAxis(0, 1, "also_broken(".into()),
    );
    assert!(app.issues.contains_key(&InputKey::Control(0, 0, 0)));
    assert!(app.issues.contains_key(&InputKey::Control(2, 0, 1)));
    assert!(!app.issues.contains_key(&InputKey::Control(0, 0, 1)));
}
#[test]
fn removing_and_undoing_cards_preserves_collapse_state_and_typing_is_one_edit() {
    let mut app = super::super::tests::app(Mode::Real);
    app.busy = true;
    let _ = app.update(Message::AddPath(InputKind::Interval));
    let _ = app.update(Message::AddPath(InputKind::Point));
    let _ = app.update(Message::TogglePath(2));
    let original = app.project.clone();
    for value in ["-", "-1", "-1.5"] {
        send(&mut app, 0, Message::Field(Field::Start, value.into()));
    }
    let typed = app.project.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.project, original,
        "typing into a non-selected card must coalesce from its first edit"
    );
    let _ = app.update(Message::Redo);
    assert_eq!(app.project, typed);
    send(&mut app, 1, Message::RemovePath);
    assert_eq!(app.project.settings.additional_paths.len(), 1);
    assert!(app.collapsed_paths.contains(&1));
    assert!(!app.collapsed_paths.contains(&2));
    let _ = app.update(Message::Undo);
    assert_eq!(app.project, typed);
    assert!(app.collapsed_paths.contains(&2));
    assert!(!app.collapsed_paths.contains(&1));
}
