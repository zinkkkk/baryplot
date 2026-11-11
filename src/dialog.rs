//! Movable modal content with retained position and ordinary child widget/overlay routing.
use iced::{
    Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector,
    advanced::{
        Layout, Shell, Widget, layout, overlay, renderer,
        widget::{self, Tree},
    },
    mouse,
};

pub struct Dialog<W> {
    content: W,
    visible: bool,
}
impl<W> Dialog<W> {
    pub fn new(content: W, visible: bool) -> Self {
        Self { content, visible }
    }
}
#[derive(Default)]
struct State {
    offset: Vector,
    drag: Option<Drag>,
}
struct Drag {
    pointer: Point,
    offset: Vector,
}
impl<W> widget::Meta for Dialog<W> {}
impl<W, Message> Widget<Message, Theme, Renderer> for Dialog<W>
where
    W: Widget<Message, Theme, Renderer>,
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }
    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
    }
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }
    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let limits = limits.width(Length::Fill).height(Length::Fill);
        tree.size = limits.resolve(Length::Fill, Length::Fill, Size::ZERO);
        if !self.visible {
            tree.state.downcast_mut::<State>().drag = None;
            return;
        }
        self.content
            .layout(&mut tree.children[0], renderer, &limits.loose());
        let child = tree.children[0].size;
        let centre = Vector::new(
            (tree.size.width - child.width) * 0.5,
            (tree.size.height - child.height) * 0.5,
        );
        let state = tree.state.downcast_mut::<State>();
        let desired = centre + state.offset;
        let inset = 4.;
        let position = Vector::new(
            desired
                .x
                .clamp(inset, (tree.size.width - child.width - inset).max(inset)),
            desired
                .y
                .clamp(inset, (tree.size.height - child.height - inset).max(inset)),
        );
        // Preserve the actual visible offset after clamping or a window resize.
        state.offset = position - centre;
        tree.children[0].translation = position;
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if !self.visible {
            return;
        }
        let child_bounds = layout.iter(&tree.children).next().unwrap().0.bounds();
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(title_bounds(child_bounds)) =>
            {
                if let Some(pointer) = cursor.position() {
                    state.drag = Some(Drag {
                        pointer,
                        offset: state.offset,
                    });
                    shell.capture_event();
                    shell.request_redraw();
                    return;
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(drag) = &state.drag {
                    state.offset = drag.offset + (*position - drag.pointer);
                    shell.invalidate_layout();
                    shell.request_redraw();
                    shell.capture_event();
                    return;
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.drag.take().is_some() =>
            {
                shell.capture_event();
                shell.request_redraw();
                return;
            }
            Event::Window(iced::window::Event::Unfocused) => {
                state.drag = None;
            }
            _ => {}
        }
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .update(tree, event, layout, cursor, renderer, shell, viewport);
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if !self.visible {
            return;
        }
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        if !self.visible {
            return mouse::Interaction::None;
        }
        let (child, child_tree) = layout.iter(&tree.children).next().unwrap();
        if tree.state.downcast_ref::<State>().drag.is_some() {
            return mouse::Interaction::Grabbing;
        }
        if cursor.is_over(title_bounds(child.bounds())) {
            return mouse::Interaction::Grab;
        }
        self.content
            .mouse_interaction(child_tree, child, cursor, viewport, renderer)
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        if !self.visible {
            return;
        }
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .operate(tree, layout, viewport, renderer, operation);
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, Message, Theme, Renderer>> {
        if !self.visible {
            return Vec::new();
        }
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(tree, layout, renderer, viewport, translation, window)
    }
}
fn title_bounds(bounds: Rectangle) -> Rectangle {
    // Both modal kinds have 16–18 px padding. Leave the close button interactive.
    Rectangle {
        x: bounds.x + 12.,
        y: bounds.y + 10.,
        width: (bounds.width - 68.).max(0.),
        height: 36.,
    }
}

#[cfg(test)]
mod tests;
