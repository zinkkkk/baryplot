use super::*;
#[test]
fn axis_gutters_drag_one_axis_without_disturbing_the_other() {
    use canvas::Program;
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
    let bounds = Rectangle {
        x: 100.,
        y: 50.,
        width: 900.,
        height: 700.,
    };
    let a = area(bounds.size());
    for (axis, local, delta) in [
        (
            Axis::X,
            Point::new(a.x + 300., a.y + a.height + 15.),
            iced::Vector::new(90., 0.),
        ),
        (
            Axis::Y,
            Point::new(20., a.y + 200.),
            iced::Vector::new(90., 0.),
        ),
        (
            Axis::Y,
            Point::new(20., a.y + 200.),
            iced::Vector::new(0., -90.),
        ),
    ] {
        let start = local + iced::Vector::new(bounds.x, bounds.y);
        let mut state = State::default();
        let _ = plot.update(
            &mut state,
            &canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            mouse::Cursor::Available(start),
        );
        assert_eq!(state.drag.as_ref().expect("axis drag").axis, Some(axis));
        let end = start + delta;
        let _ = plot.update(
            &mut state,
            &canvas::Event::Mouse(mouse::Event::CursorMoved { position: end }),
            bounds,
            mouse::Cursor::Available(end),
        );
        let Some(Interaction::Bounds(next)) = state.pending else {
            panic!("axis update")
        };
        if axis == Axis::X {
            assert!(next.width() < project.settings.bounds.width());
            assert_eq!(next.y_min, project.settings.bounds.y_min);
            assert_eq!(next.y_max, project.settings.bounds.y_max);
        } else {
            assert!(next.height() < project.settings.bounds.height());
            assert_eq!(next.x_min, project.settings.bounds.x_min);
            assert_eq!(next.x_max, project.settings.bounds.x_max);
        }
        assert!(next.valid());
    }
}
#[test]
fn coincident_handles_keep_their_positions_and_the_emphasized_row_is_pickable() {
    let mut project = crate::model::reference_project(crate::model::Mode::Real);
    project.settings.nodes = false;
    for entry in &mut project.entries {
        entry.real_y[1] = 1.;
    }
    let model = project.compile().expect("model");
    let mut plot = Plot {
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
    let a = area(Size::new(900., 700.));
    let b = project.settings.bounds;
    for row in 0..project.entries.len() {
        plot.emphasized_entry = Some(row);
        for marker in plot.displayed_markers(b, a) {
            let (target, z, _) = plot
                .markers()
                .into_iter()
                .find(|m| m.0 == marker.target)
                .expect("source marker");
            assert_eq!(
                marker.point,
                screen(z, b, a),
                "stacking must not shift a marker"
            );
            if target_row(target) == Some(row) {
                assert_eq!(plot.hit(marker.point, b, a), Some(target));
            }
        }
    }
}
#[test]
fn derived_products_have_no_drag_target() {
    let mut project = crate::model::reference_project(crate::model::Mode::Real);
    project.settings.nodes = false;
    project.settings.values = false;
    project.settings.weights = false;
    project.settings.weighted_values = true;
    let model = project.compile().expect("model");
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
    let a = area(Size::new(900., 700.));
    let b = project.settings.bounds;
    for marker in plot.displayed_markers(b, a) {
        assert_eq!(plot.hit(marker.point, b, a), None);
    }
}
#[test]
fn wheel_zoom_preserves_world_position_under_cursor() {
    use canvas::Program;
    for mapping in [
        Mapping::default(),
        Mapping {
            x: axes::Scale::Log10,
            y: axes::Scale::Log10,
            ..Default::default()
        },
        Mapping {
            x: axes::Scale::SymLog,
            y: axes::Scale::SymLog,
            threshold: 0.1,
        },
    ] {
        let mut project = crate::model::reference_project(crate::model::Mode::Real);
        project.settings.guides.mapping = mapping;
        project.settings.bounds = mapping.fit(project.settings.bounds);
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
        let bounds = Rectangle {
            x: 100.,
            y: 50.,
            width: 900.,
            height: 700.,
        };
        let p = Point::new(640., 280.);
        let cursor = mouse::Cursor::Available(Point::new(bounds.x + p.x, bounds.y + p.y));
        for state_after_shift in ["held", "released", "unfocused"] {
            let fast = state_after_shift == "held";
            for direction in [-1., 1.] {
                let mut deltas = vec![
                    mouse::ScrollDelta::Lines {
                        x: 0.,
                        y: direction,
                    },
                    mouse::ScrollDelta::Pixels {
                        x: 0.,
                        y: direction * 40.,
                    },
                ];
                if fast {
                    deltas.push(mouse::ScrollDelta::Lines {
                        x: direction,
                        y: 0.,
                    });
                }
                for delta in deltas {
                    let mut state = State::default();
                    let _ = plot.update(
                        &mut state,
                        &canvas::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
                            iced::keyboard::Modifiers::SHIFT,
                        )),
                        bounds,
                        cursor,
                    );
                    if state_after_shift == "released" {
                        let _ = plot.update(
                            &mut state,
                            &canvas::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
                                iced::keyboard::Modifiers::default(),
                            )),
                            bounds,
                            cursor,
                        );
                    } else if state_after_shift == "unfocused" {
                        let _ = plot.update(
                            &mut state,
                            &canvas::Event::Window(iced::window::Event::Unfocused),
                            bounds,
                            cursor,
                        );
                    }
                    let action = plot
                        .update(
                            &mut state,
                            &canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }),
                            bounds,
                            cursor,
                        )
                        .expect("zoom action");
                    let (Some(Interaction::Bounds(b)), _, _) = action.into_inner() else {
                        panic!("bounds message")
                    };
                    let original = Projection::new(project.settings.bounds, mapping);
                    assert!(
                        (world(p, Projection::new(b, mapping), area(bounds.size()))
                            - world(p, original, area(bounds.size())))
                        .norm()
                            < 1e-10,
                        "zoom must stay anchored under the cursor"
                    );
                    let ratio =
                        mapping.projected(b).width() / mapping.projected(original.bounds).width();
                    let range = match (fast, direction > 0.) {
                        (true, true) => 0.5..0.6,
                        (false, true) => 0.85..0.9,
                        (true, false) => 1.8..1.9,
                        (false, false) => 1.1..1.2,
                    };
                    assert!(
                        range.contains(&ratio),
                        "{state_after_shift}, {delta:?}: span ratio {ratio}"
                    );
                }
            }
        }
    }
}
