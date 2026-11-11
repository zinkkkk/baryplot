use super::*;
#[test]
fn editor_navigation_does_not_invalidate_a_fit_and_typing_undo_redo_syncs_source() {
    use iced::widget::text_editor::{Action, Edit};
    let mut app = super::super::tests::app(Mode::Real);
    app.busy = true;
    for (entry, x) in app.project.entries.iter_mut().zip(["-2", "0", "2"]) {
        entry.expressions[0] = x.into();
    }
    app.project.globals = "target(x)=x".into();
    let _ = app.update(Message::GlobalKind(
        0,
        model::DefinitionKind::NumericFunction,
    ));
    let original = app.project.clone();
    let generation = app.fit.generation;
    let _ = app.update(Message::EditNumeric(0, Action::SelectAll));
    assert_eq!(app.fit.generation, generation);
    let _ = app.update(Message::EditNumeric(
        0,
        Action::Edit(Edit::Paste(Arc::new(
            original.globals[0].expression.clone(),
        ))),
    ));
    assert_eq!(
        app.fit.generation, generation,
        "replacing text with itself must not cancel a fit"
    );
    let _ = app.update(Message::EditNumeric(0, Action::SelectAll));
    let source = "let y=x^2;\nfor k in 0..2 {y+=1;}\ny";
    let _ = app.update(Message::EditNumeric(
        0,
        Action::Edit(Edit::Paste(Arc::new(source.into()))),
    ));
    assert!(app.fit.generation > generation);
    let edited = app.project.clone();
    assert_eq!(edited.globals[0].expression.trim_end(), source);
    assert_eq!(
        app.numeric_editors[&0].content.text(),
        edited.globals[0].expression
    );
    assert_eq!(
        app.compiled
            .as_ref()
            .unwrap()
            .variables
            .call_function("target", &[C::new(3., 0.)])
            .unwrap(),
        C::new(11., 0.)
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.project, original);
    assert_eq!(
        app.numeric_editors[&0].content.text(),
        original.globals[0].expression
    );
    let _ = app.update(Message::Redo);
    assert_eq!(app.project, edited);
    assert_eq!(
        app.numeric_editors[&0].content.text(),
        edited.globals[0].expression
    );
    let _ = app.update(Message::GlobalEdit(
        0,
        GlobalField::Expression,
        "let y=;\ny".into(),
    ));
    assert!(app.issues[&InputKey::GlobalExpression(0)].contains("line 1"));
    assert_eq!(
        app.points.len(),
        3,
        "invalid source must retain plot handles"
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.project, edited);
}
