use super::*;
use iced::{
    advanced::shell,
    widget::{container, space},
};
#[test]
fn dragging_is_absolute_bounded_and_retained_across_close_and_resize() {
    let mut dialog = Dialog::new(container(space()).width(300).height(220), true);
    let renderer =
        Renderer::Secondary(iced_tiny_skia::Renderer::new(renderer::Settings::default()));
    let mut tree = Tree::new::<(), _, _>(&dialog);
    <Dialog<_> as Widget<(), Theme, Renderer>>::diff(&mut dialog, &mut tree);
    let mut viewport = Rectangle::with_size(Size::new(1000., 700.));
    let layout = |dialog: &mut Dialog<_>, tree: &mut Tree, viewport: Rectangle| {
        <Dialog<_> as Widget<(), Theme, Renderer>>::layout(
            dialog,
            tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, viewport.size()),
        );
    };
    layout(&mut dialog, &mut tree, viewport);
    let centre = tree.children[0].translation;
    let start = Point::new(centre.x + 40., centre.y + 26.);
    let send =
        |dialog: &mut Dialog<_>, tree: &mut Tree, event: Event, p: Point, viewport: Rectangle| {
            let mut bus = shell::Bus::<()>::new();
            let mut shell = Shell::new(&iced::window::Headless, shell::Waker::noop(), &mut bus);
            dialog.update(
                tree,
                &event,
                Layout::new(viewport.size()),
                mouse::Cursor::Available(p),
                &renderer,
                &mut shell,
                &viewport,
            );
        };
    send(
        &mut dialog,
        &mut tree,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        start,
        viewport,
    );
    let end = start + Vector::new(-200., 50.);
    send(
        &mut dialog,
        &mut tree,
        Event::Mouse(mouse::Event::CursorMoved { position: end }),
        end,
        viewport,
    );
    layout(&mut dialog, &mut tree, viewport);
    let moved = tree.children[0].translation;
    assert_eq!(moved, centre + Vector::new(-200., 50.));
    // Relayout must not feed the moving header's local coordinates back into the drag.
    send(
        &mut dialog,
        &mut tree,
        Event::Mouse(mouse::Event::CursorMoved { position: end }),
        end,
        viewport,
    );
    layout(&mut dialog, &mut tree, viewport);
    assert_eq!(tree.children[0].translation, moved);
    send(
        &mut dialog,
        &mut tree,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        end,
        viewport,
    );
    dialog.visible = false;
    layout(&mut dialog, &mut tree, viewport);
    dialog.visible = true;
    layout(&mut dialog, &mut tree, viewport);
    assert_eq!(tree.children[0].translation, moved);
    let start = Point::new(moved.x + 40., moved.y + 26.);
    send(
        &mut dialog,
        &mut tree,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        start,
        viewport,
    );
    let edge = Point::new(-5000., 5000.);
    send(
        &mut dialog,
        &mut tree,
        Event::Mouse(mouse::Event::CursorMoved { position: edge }),
        edge,
        viewport,
    );
    layout(&mut dialog, &mut tree, viewport);
    assert_eq!(tree.children[0].translation, Vector::new(4., 476.));
    send(
        &mut dialog,
        &mut tree,
        Event::Window(iced::window::Event::Unfocused),
        edge,
        viewport,
    );
    assert!(tree.state.downcast_ref::<State>().drag.is_none());
    viewport = Rectangle::with_size(Size::new(500., 350.));
    layout(&mut dialog, &mut tree, viewport);
    assert!(tree.children[0].translation.y + tree.children[0].size.height <= viewport.height - 4.);
    let b = tree.children[0].translation;
    let close = Point::new(b.x + 280., b.y + 26.);
    send(
        &mut dialog,
        &mut tree,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        close,
        viewport,
    );
    assert!(
        tree.state.downcast_ref::<State>().drag.is_none(),
        "close buttons must not start a drag"
    );
}
