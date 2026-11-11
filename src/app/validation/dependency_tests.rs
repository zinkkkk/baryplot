use super::*;
#[test]
fn curve_edits_keep_the_domain_texture_and_do_not_schedule_colour_work() {
    let mut app = super::super::tests::app(Mode::Complex);
    let image = iced::widget::image::Handle::from_rgba(1, 1, vec![1, 2, 3, 255]);
    let mut data = (**app.data.as_ref().expect("data")).clone();
    data.image = Some(image.clone());
    app.data = Some(Arc::new(data));
    app.domain_pending = false;
    app.dirty = false;
    let domain_revision = app.domain_revision;
    let epoch = app.latest.load(Ordering::Relaxed);
    for message in [
        Message::Curve(Curve::Nurbs),
        Message::ControlAxis(0, 0, "-3".into()),
        Message::ControlWeight(1, "2".into()),
        Message::Plot(Interaction::Move(Target::Control(2), C::new(1.5, -0.5))),
        Message::Field(Field::Samples, "2048".into()),
        Message::Field(Field::Extrapolation, "25".into()),
    ] {
        let _ = app.update(message);
        assert!(app.error.is_none());
        assert_eq!(app.domain_revision, domain_revision);
        assert_eq!(app.latest.load(Ordering::Relaxed), epoch);
        assert!(!app.domain_pending);
        assert!(!app.busy);
        assert_eq!(
            app.data.as_ref().expect("data").image.as_ref(),
            Some(&image)
        );
    }
    let _ = app.update(Message::EditImaginary(0, 1, "2".into()));
    assert!(
        app.domain_revision > domain_revision,
        "changing function parameters must still update domain colours"
    );
    app.latest.fetch_add(1, Ordering::Relaxed); // Cancel the test's newly dispatched colour job.
}
