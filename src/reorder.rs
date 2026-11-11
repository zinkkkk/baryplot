//! Animated row reordering. Pointer motion never rebuilds numerical state or changes the model.
use crate::app::Message;
use iced::{
    Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Renderer, Shadow, Size,
    Theme, Transformation, Vector,
    advanced::{
        Layout, Renderer as _, Shell, Widget, layout, overlay, renderer,
        text::{self, Renderer as _},
        widget::{self, Tree},
    },
    mouse,
};
use std::time::Instant;

const SPACING: f32 = 12.;
const ANIMATION_SECONDS: f32 = 0.035;
const EDGE_ZONE: f32 = 64.;
// A held pointer may cross into the header/footer without losing edge scrolling.
// This does not expand the region where a row can be dropped.
const EDGE_OVERSHOOT: f32 = 24.;
const MIN_SCROLL_SPEED: f32 = 160.;
const MAX_SCROLL_SPEED: f32 = 700.;
const LEFT_DRAG_MARGIN: f32 = 64.;

pub struct Item<'a> {
    pub id: usize,
    pub content: Element<'a, Message>,
    pub name: String,
    pub values: [String; 3],
    pub color: Color,
}
pub struct Rows<'a> {
    items: Vec<Item<'a>>,
    dragging: Option<usize>,
}
impl<'a> Rows<'a> {
    pub fn new(items: Vec<Item<'a>>, dragging: Option<usize>) -> Self {
        Self { items, dragging }
    }
}
#[derive(Default)]
struct State {
    keys: Vec<usize>,
    motion: Vec<Motion>,
    drag: Option<Drag>,
    settling: Option<usize>,
    last_frame: Option<Instant>,
    height: f32,
    target_height: f32,
}
#[derive(Default)]
struct Motion {
    y: f32,
    target: f32,
    height: f32,
}
#[derive(Clone)]
struct Drag {
    id: usize,
    slot: usize,
    pointer: Point,
    grab: Vector,
    lift: f32,
    valid: bool,
}
impl State {
    fn targets(&mut self) {
        let mut y = 0.;
        if let Some(drag) = &self.drag {
            let Some(source) = self.keys.iter().position(|&id| id == drag.id) else {
                return;
            };
            let gap = placeholder_height(self.motion[source].height) + SPACING;
            let mut slot = 0;
            for (i, m) in self.motion.iter_mut().enumerate() {
                if i == source {
                    continue;
                }
                if slot == drag.slot {
                    y += gap;
                }
                m.target = y;
                y += m.height + SPACING;
                slot += 1;
            }
            self.motion[source].target = if drag.slot == slot {
                y
            } else {
                self.motion
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != source)
                    .nth(drag.slot)
                    .map_or(y, |(_, m)| m.target - gap)
            };
        } else {
            for m in &mut self.motion {
                m.target = y;
                y += m.height + SPACING;
            }
        }
        self.target_height = self.motion.iter().map(|m| m.height).sum::<f32>()
            + SPACING * self.motion.len().saturating_sub(1) as f32;
        if let Some(drag) = &self.drag
            && let Some(source) = self.keys.iter().position(|&id| id == drag.id)
        {
            self.target_height -=
                self.motion[source].height - placeholder_height(self.motion[source].height);
        }
    }
    fn point(&mut self, pointer: Option<Point>, bounds: Rectangle, viewport: Rectangle) {
        let Some(drag) = &mut self.drag else { return };
        let Some(p) = pointer else {
            drag.valid = false;
            return;
        };
        drag.pointer = p;
        drag.valid = drop_zone(bounds, viewport).is_some_and(|zone| zone.contains(p));
        if !drag.valid {
            return;
        }
        let Some(source) = self.keys.iter().position(|&id| id == drag.id) else {
            return;
        };
        let centre =
            p.y - bounds.y - drag.grab.y + placeholder_height(self.motion[source].height) * 0.5;
        // Hit-test target positions, rather than moving pixels. The reserved gap
        // provides hysteresis so an animating neighbour cannot flip the target.
        drag.slot = self
            .motion
            .iter()
            .enumerate()
            .filter(|(i, m)| *i != source && centre > m.target + m.height * 0.5)
            .count();
        self.targets();
    }
    fn step(&mut self, now: Instant) -> (bool, f32) {
        let dt = self.last_frame.replace(now).map_or(1. / 60., |last| {
            now.saturating_duration_since(last).as_secs_f32().min(0.05)
        });
        let blend = 1. - (-dt / ANIMATION_SECONDS).exp();
        let mut moving = false;
        if (self.target_height - self.height).abs() > 0.15 {
            self.height += (self.target_height - self.height) * blend;
            moving = true;
        } else {
            self.height = self.target_height;
        }
        for m in &mut self.motion {
            if (m.target - m.y).abs() > 0.15 {
                m.y += (m.target - m.y) * blend;
                moving = true;
            } else {
                m.y = m.target;
            }
        }
        if let Some(drag) = &mut self.drag
            && drag.lift < 0.999
        {
            drag.lift += (1. - drag.lift) * blend;
            moving = true;
        }
        if !moving {
            self.settling = None;
        }
        (moving, dt)
    }
    fn finish(&mut self, bounds: Rectangle, viewport: Rectangle) -> Option<(usize, usize)> {
        let drag = self.drag.take()?;
        if let Some(source) = self.keys.iter().position(|&id| id == drag.id) {
            let size = Size::new(bounds.width, self.motion[source].height);
            self.motion[source].y = ghost_bounds(&drag, size, viewport).y - bounds.y;
            self.settling = Some(drag.id);
        }
        self.targets();
        drag.valid.then_some((drag.id, drag.slot))
    }
    fn sync(&self, children: &mut [Tree]) {
        for (m, child) in self.motion.iter().zip(children) {
            child.translation = Vector::new(0., m.y);
        }
    }
}
impl widget::Meta for Rows<'_> {}
impl Widget<Message, Theme, Renderer> for Rows<'_> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }
    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fit)
    }
    fn diff(&mut self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<State>();
        let keys: Vec<_> = self.items.iter().map(|i| i.id).collect();
        if keys != state.keys {
            let mut old: std::collections::BTreeMap<_, _> = std::mem::take(&mut state.keys)
                .into_iter()
                .zip(std::mem::take(&mut tree.children))
                .zip(std::mem::take(&mut state.motion))
                .map(|((key, tree), motion)| (key, (tree, motion)))
                .collect();
            let same_rows = old.len() == keys.len() && keys.iter().all(|key| old.contains_key(key));
            if !same_rows {
                state.drag = None;
                state.settling = None;
                state.last_frame = None;
            }
            for item in &self.items {
                let (child, motion) = old
                    .remove(&item.id)
                    .unwrap_or_else(|| (Tree::new(&item.content), Motion::default()));
                tree.children.push(child);
                state
                    .motion
                    .push(if same_rows { motion } else { Motion::default() });
            }
            state.keys = keys;
        }
        if state
            .drag
            .as_ref()
            .is_some_and(|drag| Some(drag.id) != self.dragging)
        {
            state.drag = None;
            state.targets();
        }
        for (item, child) in self.items.iter_mut().zip(&mut tree.children) {
            child.diff(&mut item.content);
        }
    }
    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let width = limits
            .width(Length::Fill)
            .resolve(Length::Fill, Length::Fit, Size::ZERO)
            .width;
        let child_limits =
            layout::Limits::new(Size::new(width, 0.), Size::new(width, f32::INFINITY));
        let state = tree.state.downcast_mut::<State>();
        let mut y = 0.;
        for ((item, child), m) in self
            .items
            .iter_mut()
            .zip(&mut tree.children)
            .zip(&mut state.motion)
        {
            item.content.layout(child, renderer, &child_limits);
            if m.height == 0. {
                m.y = y;
            }
            m.height = child.size.height;
            y += child.size.height + SPACING;
        }
        state.targets();
        if state.last_frame.is_none() && state.drag.is_none() {
            for m in &mut state.motion {
                m.y = m.target;
            }
        }
        if state.drag.is_none() && state.settling.is_none() {
            state.height = state.target_height;
        }
        state.sync(&mut tree.children);
        tree.size = Size::new(width, state.height);
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
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if state.drag.is_none() =>
            {
                let hit = layout
                    .iter(&tree.children)
                    .enumerate()
                    .find(|(_, (layout, _))| {
                        cursor.is_over(grip(layout.bounds())) && cursor.is_over(*viewport)
                    });
                if let Some((index, (child, _))) = hit {
                    let pointer = cursor.position().unwrap();
                    let id = self.items[index].id;
                    state.drag = Some(Drag {
                        id,
                        slot: index,
                        pointer,
                        grab: pointer - child.position(),
                        lift: 0.,
                        valid: true,
                    });
                    state.settling = None;
                    state.last_frame = None;
                    state.targets();
                    shell.publish(Message::BeginEntryDrag(id));
                    shell.capture_event();
                    shell.request_redraw();
                    return;
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.drag.is_some() => {
                state.point(cursor.observe().position(), bounds, *viewport);
                shell.capture_event();
                shell.request_redraw();
                return;
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.drag.is_some() =>
            {
                state.point(cursor.observe().position(), bounds, *viewport);
                if let Some((id, slot)) = state.finish(bounds, *viewport) {
                    shell.publish(Message::ReorderEntry(id, slot));
                } else {
                    shell.publish(Message::EndEntryDrag);
                }
                shell.capture_event();
                shell.request_redraw();
                return;
            }
            Event::Mouse(mouse::Event::CursorLeft) if state.drag.is_some() => {
                // Leaving the window during a held drag must not discard it;
                // re-entry can resume it, and release/focus loss still cancels safely.
                if let Some(drag) = &mut state.drag {
                    drag.valid = false;
                }
                shell.request_redraw();
                return;
            }
            Event::Window(iced::window::Event::Unfocused) if state.drag.is_some() => {
                if let Some(drag) = &mut state.drag {
                    drag.valid = false;
                }
                state.finish(bounds, *viewport);
                shell.publish(Message::EndEntryDrag);
                shell.request_redraw();
            }
            Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. })
                if state.drag.is_some() =>
            {
                if *key == iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) {
                    if let Some(drag) = &mut state.drag {
                        drag.valid = false;
                    }
                    state.finish(bounds, *viewport);
                    shell.publish(Message::EndEntryDrag);
                    shell.request_redraw();
                }
                shell.capture_event();
                return;
            }
            Event::Window(iced::window::Event::RedrawRequested(now)) => {
                let pointer = cursor.observe().position();
                state.point(pointer, bounds, *viewport);
                let (moving, dt) = state.step(*now);
                state.sync(&mut tree.children);
                if tree.size.height != state.height {
                    tree.size.height = state.height;
                    shell.invalidate_layout();
                }
                let speed = state
                    .drag
                    .as_ref()
                    .and(pointer)
                    .map_or(0., |p| scroll_speed(p, bounds, *viewport));
                if speed * dt != 0. {
                    shell.publish(Message::ScrollEntries(speed * dt));
                }
                if moving || speed != 0. {
                    shell.request_redraw();
                }
                if state.drag.is_some() {
                    return;
                }
            }
            _ => {}
        }
        if state.drag.is_some() {
            return;
        }
        for (item, (layout, child)) in self
            .items
            .iter_mut()
            .zip(layout.iter_mut(&mut tree.children))
        {
            item.content
                .update(child, event, layout, cursor, renderer, shell, viewport);
        }
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
        let state = tree.state.downcast_ref::<State>();
        let Some(clip) = layout.bounds().intersection(viewport) else {
            return;
        };
        renderer.with_layer(clip, |renderer| {
            for (item, (child_layout, child)) in self.items.iter().zip(layout.iter(&tree.children))
            {
                if state.drag.as_ref().is_some_and(|d| d.id == item.id)
                    || state.settling == Some(item.id)
                {
                    continue;
                }
                if child_layout.bounds().intersects(&clip) {
                    item.content
                        .draw(child, renderer, theme, style, child_layout, cursor, &clip);
                }
            }
            if let Some(id) = state.settling
                && let Some(index) = self.items.iter().position(|i| i.id == id)
            {
                let (child_layout, child) = layout.iter(&tree.children).nth(index).unwrap();
                self.items[index].content.draw(
                    child,
                    renderer,
                    theme,
                    style,
                    child_layout,
                    cursor,
                    &clip,
                );
            }
            if let Some(drag) = &state.drag
                && let Some(index) = self.items.iter().position(|i| i.id == drag.id)
            {
                let m = &state.motion[index];
                let item = &self.items[index];
                let gap = Rectangle::new(
                    layout.position() + Vector::new(0., m.target),
                    Size::new(layout.size().width, placeholder_height(m.height)),
                );
                if drag.valid {
                    quad(
                        renderer,
                        gap,
                        alpha(item.color, 0.055 * drag.lift),
                        alpha(item.color, 0.45),
                        1.,
                        8.,
                        None,
                    );
                }
            }
        });
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if let Some(drag) = &state.drag {
            return if drag.valid {
                mouse::Interaction::Grabbing
            } else {
                mouse::Interaction::NoDrop
            };
        }
        if !cursor.is_over(*viewport) {
            return mouse::Interaction::None;
        }
        for (item, (layout, child)) in self.items.iter().zip(layout.iter(&tree.children)) {
            if cursor.is_over(grip(layout.bounds())) {
                return mouse::Interaction::Grab;
            }
            let interaction = item
                .content
                .mouse_interaction(child, layout, cursor, viewport, renderer);
            if interaction != mouse::Interaction::None {
                return interaction;
            }
        }
        mouse::Interaction::None
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        operation.container(None, layout.bounds(), viewport);
        operation.traverse(&mut |operation| {
            for (item, (layout, child)) in self
                .items
                .iter_mut()
                .zip(layout.iter_mut(&mut tree.children))
            {
                item.content
                    .operate(child, layout, viewport, renderer, operation);
            }
        });
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
        let state = tree.state.downcast_ref::<State>();
        if let Some(drag) = &state.drag
            && let Some(index) = self.items.iter().position(|item| item.id == drag.id)
        {
            let item = &self.items[index];
            let mut drag = drag.clone();
            drag.pointer += translation;
            let viewport = (*viewport + translation)
                .intersection(&Rectangle::with_size(window))
                .unwrap_or_default();
            return vec![overlay::Element::new(Box::new(Preview {
                name: &item.name,
                values: &item.values,
                color: item.color,
                drag,
                size: Size::new(layout.size().width, state.motion[index].height),
                viewport,
            }))];
        }
        self.items
            .iter_mut()
            .zip(layout.iter_mut(&mut tree.children))
            .flat_map(|(item, (layout, child))| {
                item.content
                    .overlay(child, layout, renderer, viewport, translation, window)
            })
            .collect()
    }
}
fn placeholder_height(row_height: f32) -> f32 {
    row_height / 3.
}
fn grip(bounds: Rectangle) -> Rectangle {
    Rectangle::new(bounds.position() + Vector::new(8., 8.), Size::new(28., 26.))
}
fn drop_zone(bounds: Rectangle, viewport: Rectangle) -> Option<Rectangle> {
    let zone = Rectangle {
        x: bounds.x - 16.,
        y: bounds.y,
        width: bounds.width + 32.,
        height: bounds.height,
    }
    .intersection(&viewport)?;
    // The grip is close to the window's left edge. Apply its extra tolerance
    // after clipping, otherwise the scroll viewport silently removes the margin.
    Some(Rectangle {
        x: zone.x - LEFT_DRAG_MARGIN,
        width: zone.width + LEFT_DRAG_MARGIN,
        ..zone
    })
}
fn scroll_speed(pointer: Point, bounds: Rectangle, viewport: Rectangle) -> f32 {
    let Some(visible) = drop_zone(bounds, viewport) else {
        return 0.;
    };
    if !pointer.x.is_finite()
        || !pointer.y.is_finite()
        || pointer.x < visible.x
        || pointer.x > visible.x + visible.width
        || pointer.y < viewport.y - EDGE_OVERSHOOT
        || pointer.y > viewport.y + viewport.height + EDGE_OVERSHOOT
    {
        return 0.;
    }
    // Keep a usable minimum rate as soon as the pointer enters the zone, then
    // gently accelerate towards the edge. Use viewport coordinates, not the
    // moving card or ghost, so a stationary pointer keeps scrolling.
    let zone = EDGE_ZONE.min(viewport.height * 0.5);
    if zone <= 0. {
        return 0.;
    }
    let rate = |depth: f32| {
        MIN_SCROLL_SPEED
            + (MAX_SCROLL_SPEED - MIN_SCROLL_SPEED) * (depth / zone).clamp(0., 1.).powi(2)
    };
    if pointer.y < viewport.y + zone && bounds.y < viewport.y - 0.5 {
        -rate(viewport.y + zone - pointer.y)
    } else if pointer.y > viewport.y + viewport.height - zone
        && bounds.y + bounds.height > viewport.y + viewport.height + 0.5
    {
        rate(pointer.y - (viewport.y + viewport.height - zone))
    } else {
        0.
    }
}
fn ghost_bounds(drag: &Drag, size: Size, viewport: Rectangle) -> Rectangle {
    let scale = 1. - 0.055 * drag.lift;
    let width = size.width * scale;
    let height = (size.height * 0.84).clamp(96., 144.) * scale;
    let x = (drag.pointer.x - drag.grab.x * scale + 7. * drag.lift).clamp(
        viewport.x + 3.,
        (viewport.x + viewport.width - width - 3.).max(viewport.x + 3.),
    );
    let y = drag.pointer.y - drag.grab.y * scale;
    Rectangle::new(Point::new(x, y), Size::new(width, height))
}
struct Preview<'a> {
    name: &'a str,
    values: &'a [String; 3],
    color: Color,
    drag: Drag,
    size: Size,
    viewport: Rectangle,
}
impl overlay::Overlay<Message, Theme, Renderer> for Preview<'_> {
    fn draw(&self, renderer: &mut Renderer, _: &Theme, _: &renderer::Style, _: mouse::Cursor) {
        renderer.with_layer(self.viewport, |renderer| ghost(renderer, self));
    }
}
fn ghost(renderer: &mut Renderer, preview: &Preview<'_>) {
    let Preview {
        name,
        values,
        color,
        drag,
        size,
        viewport,
    } = preview;
    let bounds = ghost_bounds(drag, *size, *viewport);
    let scale = 1. - 0.055 * drag.lift;
    let local = Rectangle::with_size(Size::new(bounds.width / scale, bounds.height / scale));
    let opacity = if drag.valid { 0.86 } else { 0.56 };
    renderer.with_transformation(
        Transformation::translate(bounds.x, bounds.y) * Transformation::scale(scale),
        |renderer| {
            quad(
                renderer,
                local,
                Color::from_rgba(0.09, 0.12, 0.17, opacity),
                alpha(*color, opacity),
                1.5,
                10.,
                Some(Shadow {
                    color: Color::from_rgba(0., 0., 0., 0.35),
                    offset: Vector::new(0., 8.),
                    blur_radius: 20.,
                }),
            );
            ink(
                renderer,
                format!("≡  {}", name),
                Rectangle::new(Point::new(12., 10.), Size::new(local.width - 24., 22.)),
                alpha(*color, 0.95),
                14.,
                true,
            );
            let w = (local.width - 24.) / 3.;
            for (i, label) in ["Node · n", "Value · v", "Weight · w"]
                .into_iter()
                .enumerate()
            {
                let x = 12. + i as f32 * w;
                ink(
                    renderer,
                    label.into(),
                    Rectangle::new(Point::new(x, 39.), Size::new(w - 6., 17.)),
                    Color::from_rgba(0.67, 0.74, 0.84, 0.9),
                    10.,
                    false,
                );
                ink(
                    renderer,
                    values[i].clone(),
                    Rectangle::new(Point::new(x, 57.), Size::new(w - 6., 21.)),
                    Color::from_rgba(0.9, 0.94, 1., 0.92),
                    12.,
                    false,
                );
            }
            ink(
                renderer,
                if drag.valid {
                    "Release to place · Esc cancels"
                } else {
                    "Move over the list to drop · Esc cancels"
                }
                .into(),
                Rectangle::new(
                    Point::new(12., local.height - 23.),
                    Size::new(local.width - 24., 18.),
                ),
                alpha(*color, 0.85),
                10.,
                false,
            );
        },
    );
}
fn alpha(mut color: Color, a: f32) -> Color {
    color.a = a;
    color
}
fn quad(
    renderer: &mut Renderer,
    bounds: Rectangle,
    fill: Color,
    border: Color,
    width: f32,
    radius: f32,
    shadow: Option<Shadow>,
) {
    renderer.fill_quad(
        renderer::Quad {
            bounds,
            border: Border {
                color: border,
                width,
                radius: radius.into(),
            },
            shadow: shadow.unwrap_or_default(),
            ..Default::default()
        },
        fill,
    );
}
fn ink(
    renderer: &mut Renderer,
    content: String,
    bounds: Rectangle,
    color: Color,
    size: f32,
    bold: bool,
) {
    renderer.fill_text(
        text::Text {
            content,
            bounds: bounds.size(),
            size: Pixels(size),
            line_height: Default::default(),
            font: Font {
                weight: if bold {
                    iced::font::Weight::Bold
                } else {
                    iced::font::Weight::Normal
                },
                ..Font::DEFAULT
            },
            align_x: Default::default(),
            align_y: iced::alignment::Vertical::Top,
            shaping: Default::default(),
            wrapping: text::Wrapping::None,
            ellipsis: text::Ellipsis::End,
            hint_factor: None,
        },
        bounds.position(),
        color,
        bounds,
    );
}

#[cfg(test)]
mod tests;
