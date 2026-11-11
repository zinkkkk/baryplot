use super::*;
use iced::Widget as _;
use iced::{
    advanced::shell,
    widget::{container, space},
};
struct Harness {
    rows: Rows<'static>,
    tree: Tree,
    renderer: Renderer,
    viewport: Rectangle,
}
impl Harness {
    fn new() -> Self {
        let mut rows = Rows::new(
            [100., 120., 90.]
                .into_iter()
                .enumerate()
                .map(|(id, height)| Item {
                    id,
                    content: container(space())
                        .height(height)
                        .width(Length::Fill)
                        .boxed(),
                    name: format!("p{id}"),
                    values: ["1".into(), "2".into(), "3".into()],
                    color: Color::from_rgb(0.4, 0.9, 0.8),
                })
                .collect(),
            None,
        );
        let renderer =
            Renderer::Secondary(iced_tiny_skia::Renderer::new(renderer::Settings::default()));
        let mut tree = Tree::new(&rows);
        rows.diff(&mut tree);
        rows.layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(400., 600.)),
        );
        Self {
            rows,
            tree,
            renderer,
            viewport: Rectangle::new(Point::new(20., 20.), Size::new(400., 600.)),
        }
    }
    fn send(&mut self, event: Event, p: Point) -> Vec<Message> {
        self.send_cursor(event, mouse::Cursor::Available(p))
    }
    fn send_cursor(&mut self, event: Event, cursor: mouse::Cursor) -> Vec<Message> {
        let mut bus = shell::Bus::new();
        let mut shell = Shell::new(&iced::window::Headless, shell::Waker::noop(), &mut bus);
        let layout = Layout::new(self.tree.size).move_to(self.viewport.position());
        self.rows.update(
            &mut self.tree,
            &event,
            layout,
            cursor,
            &self.renderer,
            &mut shell,
            &self.viewport,
        );
        bus.drain().map(|(m, _)| m).collect()
    }
    fn press(&mut self, index: usize) -> Point {
        let p = Point::new(40., 40. + self.tree.children[index].translation.y);
        let messages = self.send(
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            p,
        );
        assert!(matches!(messages.as_slice(),[Message::BeginEntryDrag(id)] if *id==index));
        self.rows.dragging = Some(index);
        p
    }
    fn move_to(&mut self, p: Point) {
        assert!(
            self.send(Event::Mouse(mouse::Event::CursorMoved { position: p }), p)
                .is_empty()
        );
    }
    fn frame(&mut self, p: Point, time: Instant) -> Vec<Message> {
        self.send(Event::Window(iced::window::Event::RedrawRequested(time)), p)
    }
    fn state(&self) -> &State {
        self.tree.state.downcast_ref()
    }
}
#[test]
fn previews_use_stable_slots_and_smoothly_move_neighbours_before_commit() {
    let mut h = Harness::new();
    h.press(0);
    let point = Point::new(80., 230.);
    h.move_to(point);
    assert_eq!(h.state().drag.as_ref().unwrap().slot, 1);
    assert_eq!(h.state().motion[1].target, 0.);
    assert_eq!(h.state().motion[0].target, 132.);
    let start = Instant::now();
    h.frame(point, start);
    assert!(h.state().motion[1].y > 0. && h.state().motion[1].y < 112.);
    for frame in 1..25 {
        h.move_to(point);
        h.frame(point, start + std::time::Duration::from_millis(frame * 16));
        assert_eq!(
            h.state().drag.as_ref().unwrap().slot,
            1,
            "moving neighbours must not change a stationary pointer's target"
        );
    }
    assert!(h.state().motion[1].y < 0.2);
    let drag = h.state().drag.as_ref().unwrap();
    let ghost = ghost_bounds(drag, Size::new(400., 100.), h.viewport);
    assert!(ghost.width < 400. && ghost.height < 100.);
    let messages = h.send(
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        point,
    );
    assert!(matches!(messages.as_slice(), [Message::ReorderEntry(0, 1)]));
    assert!(h.state().drag.is_none());
}
#[test]
fn blank_gaps_and_both_ends_are_valid_and_outside_release_and_escape_cancel() {
    let mut h = Harness::new();
    h.press(2);
    let gap = Point::new(110., 125.); // Between the first two cards; no card hover event.
    h.move_to(gap);
    let result = h.send(
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        gap,
    );
    assert!(matches!(result.as_slice(), [Message::ReorderEntry(2, 1)]));
    for (source, point, slot) in [(2, Point::new(80., 23.), 0), (0, Point::new(80., 352.), 2)] {
        let mut h = Harness::new();
        h.press(source);
        h.move_to(point);
        let result = h.send(
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            point,
        );
        assert!(
            matches!(result.as_slice(),[Message::ReorderEntry(id,index)] if *id==source && *index==slot)
        );
    }
    let mut h = Harness::new();
    h.press(0);
    let outside = Point::new(600., 250.);
    h.move_to(outside);
    assert!(!h.state().drag.as_ref().unwrap().valid);
    let result = h.send(
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        outside,
    );
    assert!(matches!(result.as_slice(), [Message::EndEntryDrag]));
    let mut h = Harness::new();
    let p = h.press(0);
    let result = h.send(Event::Window(iced::window::Event::Unfocused), p);
    assert!(matches!(result.as_slice(), [Message::EndEntryDrag]));
    assert!(h.state().drag.is_none());
    let mut h = Harness::new();
    let p = h.press(0);
    let key = iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape);
    let result = h.send(
        Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Escape),
            location: iced::keyboard::Location::Standard,
            modifiers: Default::default(),
            text: None,
            repeat: false,
        }),
        p,
    );
    assert!(matches!(result.as_slice(), [Message::EndEntryDrag]));
    assert!(h.state().drag.is_none());
}
#[test]
fn left_margin_accepts_obstructed_positions_and_window_exit_can_resume() {
    let mut h = Harness::new();
    h.press(0);
    let left = Point::new(h.viewport.x - 24., 230.);
    let messages = h.send_cursor(
        Event::Mouse(mouse::Event::CursorMoved { position: left }),
        mouse::Cursor::Obstructed(left),
    );
    assert!(messages.is_empty());
    assert!(
        h.state().drag.as_ref().unwrap().valid,
        "scroll clipping must not remove the left margin"
    );
    let messages = h.send_cursor(
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        mouse::Cursor::Obstructed(left),
    );
    assert!(matches!(messages.as_slice(), [Message::ReorderEntry(0, 1)]));
    let bounds = Rectangle::new(Point::new(20., 20.), h.tree.size);
    let zone = drop_zone(bounds, h.viewport).unwrap();
    assert_eq!(
        zone.x + zone.width,
        h.viewport.x + h.viewport.width,
        "right margin must remain the same"
    );
    let mut h = Harness::new();
    h.press(0);
    let messages = h.send_cursor(
        Event::Mouse(mouse::Event::CursorLeft),
        mouse::Cursor::Unavailable,
    );
    assert!(messages.is_empty());
    assert!(
        h.state().drag.is_some(),
        "briefly leaving the window must keep the held drag"
    );
    let p = Point::new(80., 230.);
    h.move_to(p);
    assert!(h.state().drag.as_ref().unwrap().valid);
    let messages = h.send(
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        p,
    );
    assert!(matches!(messages.as_slice(), [Message::ReorderEntry(0, 1)]));
}
#[test]
fn compact_marker_is_the_entire_reserved_space_without_trailing_or_vertical_padding() {
    let mut h = Harness::new();
    let p = h.press(1);
    let now = Instant::now();
    for frame in 0..30 {
        h.frame(p, now + std::time::Duration::from_millis(frame * 16));
    }
    let state = h.state();
    let marker = placeholder_height(state.motion[1].height);
    assert_eq!(marker, 40.);
    assert!(
        (state.motion[2].y - (state.motion[1].target + marker + SPACING)).abs() < 0.2,
        "the next card must follow the compact marker, without the old full-row gap"
    );
    assert!((h.tree.size.height - 254.).abs() < 0.2);
    assert!(
        (state.motion[2].y + state.motion[2].height - h.tree.size.height).abs() < 0.2,
        "the removed row height must not become blank space below the list"
    );
    let bounds = Rectangle::new(h.viewport.position(), h.tree.size);
    let zone = drop_zone(bounds, h.viewport).unwrap();
    assert_eq!(zone.y, bounds.y);
    assert_eq!(zone.y + zone.height, bounds.y + bounds.height);
    h.move_to(Point::new(80., bounds.y - 1.));
    assert!(!h.state().drag.as_ref().unwrap().valid);
    h.move_to(Point::new(80., bounds.y + bounds.height + 1.));
    assert!(!h.state().drag.as_ref().unwrap().valid);
    h.move_to(p);
    assert!(h.state().drag.as_ref().unwrap().valid);
}
#[test]
fn edge_scrolling_is_bounded_and_stops_at_the_ends() {
    let mut h = Harness::new();
    h.viewport.height = 220.;
    h.press(0);
    let p = Point::new(80., h.viewport.y + h.viewport.height - 2.);
    h.move_to(p);
    let messages = h.frame(p, Instant::now());
    assert!(
        matches!(messages.as_slice(),[Message::ScrollEntries(y)] if *y>0. && *y<=MAX_SCROLL_SPEED*0.05)
    );
    let bounds = Rectangle::new(Point::new(20., 20.), h.tree.size);
    assert_eq!(
        scroll_speed(Point::new(80., 21.), bounds, h.viewport),
        0.,
        "cannot scroll before the first card"
    );
    let viewport = Rectangle {
        y: bounds.y + bounds.height - 220.,
        height: 220.,
        ..h.viewport
    };
    assert_eq!(
        scroll_speed(
            Point::new(80., viewport.y + viewport.height - 1.),
            bounds,
            viewport
        ),
        0.,
        "cannot scroll past the last card"
    );
}
#[test]
fn scrolling_and_rebuilding_during_a_drag_keeps_pointer_and_row_state() {
    let mut h = Harness::new();
    h.press(0);
    let p = Point::new(80., 230.);
    h.move_to(p);
    let now = Instant::now();
    h.frame(p, now);
    let before = h.state().motion[1].y;
    h.rows.diff(&mut h.tree);
    assert_eq!(h.state().drag.as_ref().unwrap().slot, 1);
    assert_eq!(h.state().motion[1].y, before);
    // Scrollable passes both cursor and viewport in content coordinates.
    h.viewport.y += 100.;
    h.frame(
        Point::new(p.x, p.y + 100.),
        now + std::time::Duration::from_millis(16),
    );
    assert!(h.state().drag.as_ref().unwrap().pointer.y > p.y);
    assert!(h.state().drag.as_ref().unwrap().valid);
}

#[test]
fn edge_hover_has_a_gentle_minimum_rate_a_cap_and_independent_drop_bounds() {
    let bounds = Rectangle::new(Point::new(20., -200.), Size::new(400., 1500.));
    let viewport = Rectangle::new(Point::new(20., 20.), Size::new(400., 400.));
    for (y, sign) in [
        (viewport.y + EDGE_ZONE - 0.1, -1.),
        (viewport.y + viewport.height - EDGE_ZONE + 0.1, 1.),
        (viewport.y - 12., -1.),
        (viewport.y + viewport.height + 12., 1.),
    ] {
        let speed = scroll_speed(Point::new(80., y), bounds, viewport);
        assert_eq!(speed.signum(), sign);
        assert!((MIN_SCROLL_SPEED..=MAX_SCROLL_SPEED).contains(&speed.abs()));
    }
    for p in [
        Point::new(80., 200.),
        Point::new(80., viewport.y - EDGE_OVERSHOOT - 1.),
        Point::new(80., viewport.y + viewport.height + EDGE_OVERSHOOT + 1.),
        Point::new(900., viewport.y),
        Point::new(f32::NAN, viewport.y),
    ] {
        assert_eq!(scroll_speed(p, bounds, viewport), 0.);
    }
    let mut h = Harness::new();
    h.viewport.height = 220.;
    h.press(0);
    let p = Point::new(80., h.viewport.y + h.viewport.height + 12.);
    h.move_to(p);
    assert!(
        !h.state().drag.as_ref().unwrap().valid,
        "scrolling overshoot must not become a drop target"
    );
    assert!(
        h.frame(p, Instant::now())
            .iter()
            .any(|m| matches!(m,Message::ScrollEntries(y) if *y>0.))
    );
    let messages = h.send_cursor(
        Event::Window(iced::window::Event::RedrawRequested(Instant::now())),
        mouse::Cursor::Unavailable,
    );
    assert!(
        !messages
            .iter()
            .any(|m| matches!(m, Message::ScrollEntries(_))),
        "leaving the window must stop scrolling"
    );
    let messages = h.send(
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        p,
    );
    assert!(matches!(messages.as_slice(), [Message::EndEntryDrag]));
}
