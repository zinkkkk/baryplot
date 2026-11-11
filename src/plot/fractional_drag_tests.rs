use super::*;
use canvas::Program;
fn begin() -> (Project, Compiled, State, Rectangle, Point) {
    let project = crate::app::preset(Mode::Complex);
    let model = project.compile().expect("model");
    let bounds = Rectangle {
        x: 617.5,
        y: 46.75,
        width: 891.25,
        height: 837.5,
    };
    let local = screen(
        model.nodes[0].z,
        project.settings.bounds,
        area(bounds.size()),
    );
    let cursor = Point::new(local.x + bounds.x, local.y + bounds.y);
    let plot = Plot {
        project: &project,
        model: Some(&model),
        points: &model.nodes,
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
    let mut state = State::default();
    let _ = plot.update(
        &mut state,
        &canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        bounds,
        mouse::Cursor::Available(cursor),
    );
    assert!(state.drag.is_some());
    (project, model, state, bounds, cursor)
}
fn frame(
    plot: &Plot<'_>,
    state: &mut State,
    bounds: Rectangle,
    cursor: Point,
) -> Option<Interaction> {
    plot.update(
        state,
        &canvas::Event::Window(iced::window::Event::RedrawRequested(
            iced::time::Instant::now(),
        )),
        bounds,
        mouse::Cursor::Available(cursor),
    )
    .and_then(|a| a.into_inner().0)
}
fn move_to(plot: &Plot<'_>, state: &mut State, bounds: Rectangle, p: Point) -> Option<Interaction> {
    let _ = plot.update(
        state,
        &canvas::Event::Mouse(mouse::Event::CursorMoved { position: p }),
        bounds,
        mouse::Cursor::Available(p),
    );
    frame(plot, state, bounds, p)
}
#[test]
fn neighbouring_float_positions_do_not_keep_a_held_drag_active() {
    for scale in [1., 1.25, 1.5, 2., 3.] {
        let (project, model, mut state, bounds, start) = begin();
        let plot = Plot {
            project: &project,
            model: Some(&model),
            points: &model.nodes,
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
        state.pixel_scale.set(Some(scale));
        let stopped = Point::new(start.x + 80.375, start.y + 12.625);
        assert!(matches!(
            move_to(&plot, &mut state, bounds, stopped),
            Some(Interaction::Move(_, _))
        ));
        for i in 0..100 {
            let p = Point::new(
                if i % 2 == 0 {
                    stopped.x.next_up()
                } else {
                    stopped.x.next_down()
                },
                stopped.y,
            );
            assert!(
                move_to(&plot, &mut state, bounds, p).is_none(),
                "adjacent f32 cursor representations must not restart refinement at scale {scale}"
            );
        }
        let pixel = Point::new(stopped.x + 1. / scale, stopped.y);
        assert!(
            matches!(
                move_to(&plot, &mut state, bounds, pixel),
                Some(Interaction::Move(_, _))
            ),
            "a real one-pixel movement must still be accepted at scale {scale}"
        );
        assert!(state.drag.is_some());
    }
}
#[test]
fn drag_uses_window_motion_without_feedback_from_fractional_layout() {
    let (project, model, mut state, bounds, start) = begin();
    let plot = Plot {
        project: &project,
        model: Some(&model),
        points: &model.nodes,
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
    let stopped = Point::new(start.x + 80.375, start.y + 12.625);
    let _ = move_to(&plot, &mut state, bounds, stopped);
    for i in 0..20 {
        let changed = if i % 2 == 0 {
            Rectangle {
                x: bounds.x.next_up(),
                width: bounds.width.next_up(),
                ..bounds
            }
        } else {
            bounds
        };
        assert!(
            move_to(&plot, &mut state, changed, stopped).is_none(),
            "layout rounding must not move a captured point with a stationary physical cursor"
        );
    }
    let changed = Rectangle {
        x: bounds.x + 0.25,
        width: bounds.width + 0.5,
        ..bounds
    };
    let moved = Point::new(stopped.x + 1., stopped.y);
    let Some(Interaction::Move(_, z)) = move_to(&plot, &mut state, changed, moved) else {
        panic!("a real move must remain responsive after a layout change");
    };
    let a = area(bounds.size());
    let expected = model.nodes[0].z
        + C::new(
            (f64::from(moved.x) - f64::from(start.x)) / f64::from(a.width)
                * project.settings.bounds.width(),
            -(f64::from(moved.y) - f64::from(start.y)) / f64::from(a.height)
                * project.settings.bounds.height(),
        );
    assert!((z - expected).norm() < 1e-12);
}
#[test]
fn slow_subpixel_motion_accumulates_for_points_and_panning() {
    for pan in [false, true] {
        let (project, model, mut state, bounds, start) = begin();
        let plot = Plot {
            project: &project,
            model: Some(&model),
            points: &model.nodes,
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
        if pan {
            state.drag.as_mut().expect("drag").target = None;
        }
        // Each increment is below the noise radius, but the accumulated
        // movement must still update, using displacement from mouse-down.
        for i in 1..4 {
            let p = Point::new(start.x + i as f32 / 32., start.y);
            assert!(move_to(&plot, &mut state, bounds, p).is_none());
        }
        let p = Point::new(start.x + 0.25, start.y);
        let action = move_to(&plot, &mut state, bounds, p).expect("accumulated motion");
        let dx = 0.25 / f64::from(area(bounds.size()).width) * project.settings.bounds.width();
        match action {
            Interaction::Move(target, z) => {
                assert!(!pan);
                assert_eq!(target, Target::Parameter(0, 0));
                assert!((z - (model.nodes[0].z + C::new(dx, 0.))).norm() < 1e-12);
            }
            Interaction::Bounds(b) => {
                assert!(pan);
                assert!((b.x_min - (project.settings.bounds.x_min - dx)).abs() < 1e-12);
                assert!((b.x_max - (project.settings.bounds.x_max - dx)).abs() < 1e-12);
            }
            _ => panic!("unexpected drag action"),
        }
    }
}
