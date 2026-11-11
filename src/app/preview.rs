//! Display-independent rendering checks using iced's software backend.
use super::*;
use iced::{
    Rectangle, Size,
    advanced::{graphics::Viewport, renderer},
    mouse,
};
use iced_runtime::UserInterface;

fn completed_app(mode: Mode) -> App {
    let (mut app, _task) = App::new(preset(mode));
    let data = sampling::generate(
        app.compiled.as_ref().expect("model"),
        &app.project.settings,
        app.plan.as_ref().expect("plan"),
        app.latest.load(Ordering::Relaxed),
        &app.latest,
        |_| {},
    )
    .expect("render data");
    app.data = Some(Arc::new(data));
    app.busy = false;
    app.domain_pending = false;
    app.line_pending = false;
    app.dirty = false;
    app.image_revision = app.domain_revision;
    app
}
fn software_renderer() -> iced::Renderer {
    iced::Renderer::Secondary(iced_tiny_skia::Renderer::new(renderer::Settings::default()))
}
fn snapshot(app: &App, renderer: &mut iced::Renderer) -> tiny_skia::Pixmap {
    snapshot_at(app, renderer, Size::new(1520, 960))
}
fn snapshot_at(app: &App, renderer: &mut iced::Renderer, size: Size<u32>) -> tiny_skia::Pixmap {
    let logical = Size::new(size.width as f32, size.height as f32);
    let mut ui = UserInterface::build(
        app.view(),
        logical,
        iced_runtime::user_interface::Cache::default(),
        renderer,
    );
    paint_ui(&mut ui, renderer, size, mouse::Cursor::Unavailable)
}
fn paint_ui(
    ui: &mut UserInterface<'_, Message, iced::Theme, iced::Renderer>,
    renderer: &mut iced::Renderer,
    size: Size<u32>,
    cursor: mouse::Cursor,
) -> tiny_skia::Pixmap {
    let logical = Size::new(size.width as f32, size.height as f32);
    let theme = crate::theme();
    ui.draw(
        renderer,
        &theme,
        &renderer::Style {
            text_color: iced::Color::WHITE,
        },
        cursor,
    );
    let mut pixmap = tiny_skia::Pixmap::new(size.width, size.height).expect("pixmap");
    let mut mask = tiny_skia::Mask::new(size.width, size.height).expect("mask");
    let iced::Renderer::Secondary(software) = renderer else {
        unreachable!()
    };
    software.draw(
        &mut pixmap.as_mut(),
        &mut mask,
        &Viewport::with_physical_size(size, renderer::Scale::default()),
        &[Rectangle::with_size(logical)],
        theme.palette().background.base.color,
    );
    // iced's software compositor writes BGRA native window buffers.
    for pixel in pixmap.data_mut().as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    pixmap
}

fn apply_operation(
    ui: &mut UserInterface<'_, Message, iced::Theme, iced::Renderer>,
    renderer: &mut iced::Renderer,
    operation: impl iced::advanced::widget::Operation + 'static,
) {
    use iced::advanced::widget::operation::Outcome;
    let mut operation: Box<dyn iced::advanced::widget::Operation> = Box::new(operation);
    loop {
        ui.operate(renderer, operation.as_mut());
        match operation.finish() {
            Outcome::Chain(next) => operation = next,
            _ => break,
        }
    }
    ui.update(
        &iced::window::Headless,
        &iced::advanced::shell::Waker::noop(),
        &[iced::Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        ))],
        mouse::Cursor::Unavailable,
        renderer,
        &mut iced::advanced::shell::Bus::new(),
    );
}
#[derive(Default)]
struct FocusProbe {
    fields: Vec<(Option<String>, Rectangle, bool)>,
    scroll: Vec<(Rectangle, iced::Vector)>,
}
impl iced::advanced::widget::Operation for FocusProbe {
    fn traverse(&mut self, f: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation)) {
        f(self);
    }
    fn focusable(
        &mut self,
        id: Option<&iced::advanced::widget::Id>,
        bounds: Rectangle,
        state: &mut dyn iced::advanced::widget::operation::Focusable,
    ) {
        self.fields
            .push((id.map(|id| format!("{id:?}")), bounds, state.is_focused()));
    }
    fn scrollable(
        &mut self,
        _: Option<&iced::advanced::widget::Id>,
        bounds: Rectangle,
        _: Size,
        translation: iced::Vector,
        _: &mut dyn iced::advanced::widget::operation::Scrollable,
    ) {
        self.scroll.push((bounds, translation));
    }
}

#[derive(Default)]
struct RowBounds(std::collections::BTreeMap<usize, Rectangle>);
impl iced::advanced::widget::Operation for RowBounds {
    fn traverse(&mut self, f: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation)) {
        f(self);
    }
    fn container(
        &mut self,
        id: Option<&iced::advanced::widget::Id>,
        bounds: Rectangle,
        _: &Rectangle,
    ) {
        for i in 0..32 {
            if id == Some(&iced::advanced::widget::Id::from(format!("data-row-{i}"))) {
                self.0.insert(i, bounds);
                break;
            }
        }
    }
}
fn drag_row_scene(write_previews: bool, left_margin: bool) {
    use iced::{Event, Point, advanced::shell};
    let mut app = completed_app(Mode::Complex);
    app.busy = true;
    let samples = app.data.as_ref().unwrap().samples.clone();
    let domain = app.domain_revision;
    let revision = app.revision;
    let mut renderer = software_renderer();
    let size = Size::new(1520, 960);
    let logical = Size::new(1520., 960.);
    let mut ui = UserInterface::build(app.view(), logical, Default::default(), &mut renderer);
    // Focus the first row's name to check that its widget state follows its identity.
    apply_operation(
        &mut ui,
        &mut renderer,
        super::focus::navigation("input-editor", false),
    );
    let mut rows = RowBounds::default();
    ui.operate(&renderer, &mut rows);
    let first = rows.0[&0];
    let last = rows.0[&2];
    let start = Point::new(first.x + 20., first.y + 20.);
    let end = Point::new(
        if left_margin {
            first.x - 20.
        } else {
            first.x + 80.
        },
        last.y + last.height - first.height * (2. / 3.) - 10.,
    );
    let mut bus = shell::Bus::new();
    for event in [
        Event::Mouse(mouse::Event::CursorMoved { position: start }),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
    ] {
        ui.update(
            &iced::window::Headless,
            &shell::Waker::noop(),
            &[event],
            mouse::Cursor::Available(start),
            &mut renderer,
            &mut bus,
        );
    }
    assert!(
        bus.drain()
            .map(|(m, _)| m)
            .any(|m| matches!(m, Message::BeginEntryDrag(0))),
        "pressing the grip must visibly lift the row"
    );
    let cache = ui.into_cache();
    let _ = app.update(Message::BeginEntryDrag(0));
    let mut ui = UserInterface::build(app.view(), logical, cache, &mut renderer);
    ui.update(
        &iced::window::Headless,
        &shell::Waker::noop(),
        &[Event::Mouse(mouse::Event::CursorMoved { position: end })],
        mouse::Cursor::Available(end),
        &mut renderer,
        &mut bus,
    );
    let now = std::time::Instant::now();
    for frame in 0..16 {
        ui.update(
            &iced::window::Headless,
            &shell::Waker::noop(),
            &[Event::Window(iced::window::Event::RedrawRequested(
                now + std::time::Duration::from_millis(frame * 16),
            ))],
            mouse::Cursor::Available(end),
            &mut renderer,
            &mut bus,
        );
        if write_previews && [0, 3, 15].contains(&frame) {
            paint_ui(&mut ui, &mut renderer, size, mouse::Cursor::Available(end))
                .save_png(format!("/tmp/baryplot-previews/row-drag-{frame}.png"))
                .unwrap();
        }
    }
    let mut moved = RowBounds::default();
    ui.operate(&renderer, &mut moved);
    assert!(
        moved.0[&1].y < rows.0[&1].y - 50.,
        "neighbours must visibly move before release"
    );
    assert!(
        !bus.drain()
            .any(|(m, _)| matches!(m, Message::ReorderEntry(..))),
        "animation must leave the mathematical project untouched until release"
    );
    ui.update(
        &iced::window::Headless,
        &shell::Waker::noop(),
        &[Event::Mouse(mouse::Event::ButtonReleased(
            mouse::Button::Left,
        ))],
        mouse::Cursor::Available(end),
        &mut renderer,
        &mut bus,
    );
    let messages: Vec<_> = bus.drain().map(|(m, _)| m).collect();
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::ReorderEntry(0, 2)))
    );
    let cache = ui.into_cache();
    // Global release can arrive before the widget's drop; it must not discard the drop.
    let _ = app.update(Message::EndEntryDrag);
    for message in messages {
        let _ = app.update(message);
    }
    assert_eq!(app.project.ordered_entries(), vec![1, 2, 0]);
    assert_eq!(app.revision, revision);
    assert_eq!(app.domain_revision, domain);
    assert!(Arc::ptr_eq(&samples, &app.data.as_ref().unwrap().samples));
    let mut ui = UserInterface::build(app.view(), logical, cache, &mut renderer);
    for frame in 16..40 {
        ui.update(
            &iced::window::Headless,
            &shell::Waker::noop(),
            &[Event::Window(iced::window::Event::RedrawRequested(
                now + std::time::Duration::from_millis(frame * 16),
            ))],
            mouse::Cursor::Available(end),
            &mut renderer,
            &mut bus,
        );
    }
    let mut focus = FocusProbe::default();
    ui.operate(&renderer, &mut focus);
    assert_eq!(
        focus.fields.iter().position(|f| f.2),
        Some(14),
        "focus must stay attached to the moved row's name"
    );
    if write_previews {
        paint_ui(&mut ui, &mut renderer, size, mouse::Cursor::Available(end))
            .save_png("/tmp/baryplot-previews/row-drag-placed.png")
            .unwrap();
    }
    drop(ui);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.ordered_entries(), vec![0, 1, 2]);
    assert!(
        !app.history.can_undo(),
        "one complete drag is one undo entry"
    );
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.ordered_entries(), vec![1, 2, 0]);
}

fn numeric_definition_app() -> App {
    let mut app = completed_app(Mode::Real);
    app.project.globals.clear();
    app.project.globals.push(model::Definition {
        kind:model::DefinitionKind::NumericFunction,
        name:"target".into(),parameters:"x".into(),
        expression:"// Finite Fourier sum\nlet total = 0;\nfor k in 1..=12 {\n    total += sin(k*x)/k;\n}\nif re(x) < 0 {\n    -total\n} else {\n    total\n}".into(),
    });
    app.rebuild();
    app.tab = Tab::Definitions;
    app.busy = true;
    app
}

mod interaction;
mod rendering;
mod snapshots;
