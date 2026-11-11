use super::*;
use canvas::Program;
#[test]
fn pointer_bursts_publish_the_latest_position_on_the_same_render_frame() {
    let project = crate::model::reference_project(crate::model::Mode::Real);
    let plot = Plot {
        project: &project,
        model: None,
        points: &[],
        plan: None,
        data: None,
        revision: 1,
        draw_path: false,
        spectrum: None,
        spectrum_filter: Default::default(),
        editable_points: true,
        input_priority: false,
        active_path: 0,
        emphasized_entry: None,
        highlighted_entry: None,
    };
    let bounds = Rectangle::with_size(Size::new(900., 700.));
    let target = Target::Parameter(0, 1);
    let mut state = State {
        drag: Some(Drag {
            axis: None,
            target: Some(target),
            start: Point::new(400., 300.),
            bounds: project.settings.bounds,
            mapping: Mapping::default(),
            origin: world(
                Point::new(400., 300.),
                project.settings.bounds,
                area(bounds.size()),
            ),
            plot_size: area(bounds.size()).size(),
            last_pointer: Point::new(400., 300.),
        }),
        ..State::default()
    };
    let mut last = Point::ORIGIN;
    for i in 0..100 {
        last = Point::new(300. + i as f32, 300.);
        let action = plot
            .update(
                &mut state,
                &canvas::Event::Mouse(mouse::Event::CursorMoved { position: last }),
                bounds,
                mouse::Cursor::Available(last),
            )
            .expect("redraw");
        assert!(
            action.into_inner().0.is_none(),
            "raw pointer events must not flood app updates"
        );
    }
    let action = plot
        .update(
            &mut state,
            &canvas::Event::Window(iced::window::Event::RedrawRequested(
                iced::time::Instant::now(),
            )),
            bounds,
            mouse::Cursor::Available(last),
        )
        .expect("frame edit");
    let (Some(Interaction::Move(actual, value)), _, _) = action.into_inner() else {
        panic!("move message")
    };
    assert_eq!(actual, target);
    assert!((value - world(last, project.settings.bounds, area(bounds.size()))).norm() < 1e-12);
    assert!(state.pending.is_none());
    for _ in 0..3 {
        assert!(
            plot.update(
                &mut state,
                &canvas::Event::Mouse(mouse::Event::CursorMoved { position: last }),
                bounds,
                mouse::Cursor::Available(last)
            )
            .is_none(),
            "a held stationary pointer must not enqueue another edit"
        );
    }
    assert!(state.drag.is_some());
    // The final position must also commit when release comes before another frame.
    last = Point::new(500., 300.);
    let _ = plot.update(
        &mut state,
        &canvas::Event::Mouse(mouse::Event::CursorMoved { position: last }),
        bounds,
        mouse::Cursor::Available(last),
    );
    let action = plot
        .update(
            &mut state,
            &canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            bounds,
            mouse::Cursor::Available(last),
        )
        .expect("final edit");
    let Some(Interaction::Finish(Some(last_edit))) = action.into_inner().0 else {
        panic!("release must end the gesture and commit its final edit")
    };
    assert!(
        matches!(*last_edit, Interaction::Move(_,value) if (value-world(last,project.settings.bounds,area(bounds.size()))).norm()<1e-12)
    );
    assert!(state.drag.is_none());
}
