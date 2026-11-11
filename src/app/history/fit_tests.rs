use super::*;
#[test]
fn completed_fits_and_imports_undo_without_rerunning_the_solver() {
    let mut app = super::super::tests::app(Mode::Real);
    app.busy = true;
    app.project.globals = "target(x)=exp(x)".into();
    app.project.fit.function = "target".into();
    let req = crate::numerics::aaa::Request::new(&app.project).unwrap();
    let fit = crate::numerics::aaa::fit(&req, 0, &AtomicU64::new(0), |_| {})
        .unwrap()
        .unwrap();
    let original = app.project.clone();
    let generation = app.fit.generation;
    app.fit.preview = true;
    app.fit.busy = true;
    app.fit.running_generation = Some(generation);
    let _ = app.update(Message::Fitted(generation, Ok(Some(fit))));
    let result = app.fit.result.clone().unwrap();
    let _ = app.update(Message::Undo);
    assert!(app.fit.result.is_none());
    assert!(!app.fit.preview);
    assert!(!app.fit.busy);
    assert!(app.fit.pending.is_none());
    let _ = app.update(Message::Redo);
    assert!(Arc::ptr_eq(app.fit.result.as_ref().unwrap(), &result));
    assert!(app.fit.preview);
    assert!(app.fit.pending.is_none());
    let _ = app.update(Message::Fit(FitAction::Apply));
    let imported = app.project.clone();
    assert_ne!(original.entries, imported.entries);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project, original);
    assert!(app.fit.preview);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project, imported);
    assert!(!app.fit.preview);
}
