use super::*;
use canvas::Program;
#[test]
fn handles_and_pan_follow_the_pointer_on_nonlinear_axes() {
    let mut project = crate::model::reference_project(crate::model::Mode::Real);
    let s = &mut project.settings;
    s.mode = Mode::Complex;
    s.values = false;
    s.weights = false;
    s.input_path = false;
    s.guides.mapping = Mapping {
        x: axes::Scale::Log10,
        y: axes::Scale::SymLog,
        threshold: 0.2,
    };
    s.bounds = Bounds {
        x_min: 0.01,
        x_max: 10.,
        y_min: -3.,
        y_max: 3.,
    };
    let mapping = s.guides.mapping;
    let view = Projection::new(s.bounds, mapping);
    let nodes = [Node {
        row: 0,
        z: C::new(0.5, 0.4),
        value: C::default(),
        weight: C::new(1., 0.),
    }];
    let plot = Plot {
        project: &project,
        model: None,
        points: &nodes,
        plan: None,
        data: None,
        revision: 1,
        draw_path: false,
        spectrum: None,
        spectrum_filter: Default::default(),
        editable_points: true,
        input_priority: false,
        active_path: 0,
        emphasized_entry: Some(0),
        highlighted_entry: None,
    };
    let bounds = Rectangle::with_size(Size::new(900., 700.));
    let area = area(bounds.size());
    for (button, start) in [
        (mouse::Button::Left, screen(nodes[0].z, view, area)),
        (mouse::Button::Middle, Point::new(400., 300.)),
    ] {
        let mut state = State::default();
        let cursor = mouse::Cursor::Available(start);
        let _ = plot.update(
            &mut state,
            &canvas::Event::Mouse(mouse::Event::ButtonPressed(button)),
            bounds,
            cursor,
        );
        let end = start + iced::Vector::new(90., -40.);
        let _ = plot.update(
            &mut state,
            &canvas::Event::Mouse(mouse::Event::CursorMoved { position: end }),
            bounds,
            mouse::Cursor::Available(end),
        );
        match state.pending.take().unwrap() {
            Interaction::Move(Target::Parameter(0, 0), z) => {
                assert!(screen(z, view, area).distance(end) < 1e-3)
            }
            Interaction::Bounds(b) => {
                assert!(mapping.valid(b));
                assert!(
                    (world(start, view, area) - world(end, Projection::new(b, mapping), area))
                        .norm()
                        < 1e-10
                );
            }
            other => panic!("unexpected interaction: {other:?}"),
        }
    }
}
