use super::*;
fn app() -> App {
    let mut app = super::super::tests::app(Mode::Real);
    app.busy = true; // No asynchronous work is needed for history tests.
    app
}
fn send(app: &mut App, message: Message) {
    let _ = app.update(message);
}
#[test]
fn typing_invalid_drafts_and_discrete_edits_round_trip() {
    let mut app = app();
    let original = app.project.clone();
    for value in ["s", "sin(", "sin(pi/2)"] {
        send(&mut app, Message::Edit(0, 1, value.into()));
    }
    assert_eq!(app.history.undo.len(), 1);
    send(&mut app, Message::EditImaginary(0, 1, "broken(".into()));
    send(&mut app, Message::Add);
    let edited = app.project.clone();
    for _ in 0..3 {
        send(&mut app, Message::Undo);
    }
    assert_eq!(app.project, original);
    for _ in 0..3 {
        send(&mut app, Message::Redo);
    }
    assert_eq!(app.project, edited);
    send(&mut app, Message::Undo);
    send(&mut app, Message::Flag(Flag::Grid, false));
    assert!(!app.history.can_redo());
}
#[test]
fn entire_drag_and_final_pending_motion_are_one_edit() {
    let mut app = app();
    let original = app.project.clone();
    send(&mut app, Message::Plot(Interaction::BeginGesture));
    for i in 0..100 {
        send(
            &mut app,
            Message::Plot(Interaction::Move(
                Target::Parameter(0, 1),
                C::new(i as f64, 2.),
            )),
        );
    }
    send(
        &mut app,
        Message::Plot(Interaction::Finish(Some(Box::new(Interaction::Move(
            Target::Parameter(0, 1),
            C::new(123., 4.),
        ))))),
    );
    assert_eq!(app.history.undo.len(), 1);
    let final_state = app.project.clone();
    send(&mut app, Message::Undo);
    assert_eq!(app.project, original);
    send(&mut app, Message::Redo);
    assert_eq!(app.project, final_state);
}
#[test]
fn colour_preview_accept_and_cancel_keep_history_coherent() {
    use appearance::Action as A;
    let mut app = app();
    send(&mut app, Message::Flag(Flag::Grid, false));
    send(&mut app, Message::Undo);
    let original = app.project.clone();
    for accept in [false, true] {
        send(
            &mut app,
            Message::Appearance(A::Pick(crate::picker::Target::Entry(0))),
        );
        for hue in [0.1, 0.2, 0.7] {
            send(
                &mut app,
                Message::Appearance(A::PickerEdit(crate::picker::Edit::Hue(hue))),
            );
        }
        send(
            &mut app,
            Message::Appearance(if accept {
                A::PickerAccept
            } else {
                A::PickerCancel
            }),
        );
        if !accept {
            assert_eq!(app.project, original);
            assert_eq!(app.history.undo.len(), 0);
            assert!(app.history.can_redo());
        } else {
            assert_eq!(app.history.undo.len(), 1);
            assert!(!app.history.can_redo());
            let edited = app.project.clone();
            send(&mut app, Message::Undo);
            assert_eq!(app.project, original);
            send(&mut app, Message::Redo);
            assert_eq!(app.project, edited);
        }
    }
}
#[test]
fn path_edits_modes_and_clears_are_undoable_and_history_is_bounded() {
    let mut app = app();
    let original = app.project.clone();
    send(&mut app, Message::AddPath(model::InputKind::Point));
    send(&mut app, Message::ControlAxis(0, 0, "pi".into()));
    send(&mut app, Message::ClearEntries);
    send(&mut app, Message::Mode(Mode::Complex));
    let edited = app.project.clone();
    for _ in 0..4 {
        send(&mut app, Message::Undo);
    }
    assert_eq!(app.project, original);
    for _ in 0..4 {
        send(&mut app, Message::Redo);
    }
    assert_eq!(app.project, edited);
    for i in 0..150 {
        send(&mut app, Message::Flag(Flag::Grid, i % 2 == 0));
    }
    assert_eq!(app.history.undo.len(), 128);
}
