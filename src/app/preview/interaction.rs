//! Headless interaction checks. Common renderer/scene helpers live in the parent.
use super::*;

#[test]
fn leaving_card_keeps_front_order_but_clears_highlight_rings() {
    let mut app = completed_app(Mode::Real);
    app.project.settings.values = false;
    app.project.settings.weights = false;
    app.project.settings.input_path = false; // Isolate highlight rings from overlapping interval handles.
    let mut renderer = software_renderer();
    let before = snapshot_at(&app, &mut renderer, Size::new(1100, 700));
    let _ = app.update(Message::HoverEntry(0, true));
    let hovered = snapshot_at(&app, &mut renderer, Size::new(1100, 700));
    assert!(
        before.data() != hovered.data(),
        "hover decoration must appear"
    );
    let _ = app.update(Message::HoverEntry(0, false));
    assert_eq!(app.foreground_entry(), Some(0));
    let after = snapshot_at(&app, &mut renderer, Size::new(1100, 700));
    assert!(
        before.data() == after.data(),
        "hover decoration must not persist"
    );
}

#[test]
fn tab_order_visits_re_then_im_wraps_and_scrolls_offscreen_fields_into_view() {
    let mut app = completed_app(Mode::Complex);
    let mut renderer = software_renderer();
    let size = Size::new(1100., 700.);
    let mut ui = UserInterface::build(app.view(), size, Default::default(), &mut renderer);
    let mut previous = None;
    // Name, node Re/Im, value Re/Im, weight Re/Im, next row name.
    for index in 0..8 {
        apply_operation(
            &mut ui,
            &mut renderer,
            super::focus::navigation("input-editor", false),
        );
        let mut probe = FocusProbe::default();
        ui.operate(&renderer, &mut probe);
        let focused = probe
            .fields
            .iter()
            .position(|f| f.2)
            .expect("Tab must focus a field");
        assert_eq!(focused, index);
        let position = probe.fields[index].1;
        if index > 0 && index % 2 == 0 && index < 7 {
            let before: Rectangle = previous.unwrap();
            assert_eq!(
                before.x, position.x,
                "Re and Im belong to the same parameter"
            );
            assert!(position.y > before.y);
        }
        previous = Some(position);
    }
    apply_operation(
        &mut ui,
        &mut renderer,
        super::focus::navigation("input-editor", true),
    );
    let mut probe = FocusProbe::default();
    ui.operate(&renderer, &mut probe);
    assert_eq!(probe.fields.iter().position(|f| f.2), Some(6));
    // Wrap backwards from the first box to the project path at the bottom.
    for _ in 0..7 {
        apply_operation(
            &mut ui,
            &mut renderer,
            super::focus::navigation("input-editor", true),
        );
    }
    // Scroll translation is updated on draw after the operation.
    let _ = paint_ui(
        &mut ui,
        &mut renderer,
        Size::new(1100, 700),
        mouse::Cursor::Unavailable,
    );
    let mut probe = FocusProbe::default();
    ui.operate(&renderer, &mut probe);
    assert_eq!(
        probe.fields.iter().position(|f| f.2),
        Some(probe.fields.len() - 1)
    );
    let focused = probe.fields.last().unwrap().1;
    let (viewport, translation) = probe.scroll[0];
    let visible = focused - translation;
    assert!(
        visible.y >= viewport.y && visible.y + visible.height <= viewport.y + viewport.height,
        "focused box {visible:?} must be visible in {viewport:?}"
    );
    // A settings dialog has its own focus scope and cannot tab into the covered editor.
    let cache = ui.into_cache();
    let _ = app.update(Message::Appearance(appearance::Action::Open(
        appearance::Panel::Colour,
    )));
    let mut ui = UserInterface::build(app.view(), size, cache, &mut renderer);
    apply_operation(
        &mut ui,
        &mut renderer,
        super::focus::navigation("settings-dialog", false),
    );
    let mut probe = FocusProbe::default();
    ui.operate(&renderer, &mut probe);
    let focused: Vec<_> = probe.fields.iter().filter(|f| f.2).collect();
    assert_eq!(focused.len(), 1, "Tab must clear focus from covered fields");
    assert!(focused[0].1.x > 200.);
}

#[test]
fn sidebar_resize_sensor_preserves_view_proportions_and_does_not_enter_history() {
    use iced::{Event, advanced::shell};
    fn redraw(app: &mut App, renderer: &mut iced::Renderer) -> Size {
        let mut ui = UserInterface::build(
            app.view(),
            Size::new(1520., 960.),
            Default::default(),
            renderer,
        );
        let mut bus = shell::Bus::new();
        ui.update(
            &iced::window::Headless,
            &shell::Waker::noop(),
            &[Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            ))],
            mouse::Cursor::Unavailable,
            renderer,
            &mut bus,
        );
        drop(ui);
        let size = bus
            .drain()
            .find_map(|(m, _)| {
                if let Message::PlotSize(s) = m {
                    Some(s)
                } else {
                    None
                }
            })
            .expect("canvas resize notification");
        let _ = app.update(Message::PlotSize(size));
        app.plot_size.unwrap()
    }
    let mut app = completed_app(Mode::Real);
    let mut renderer = software_renderer();
    let old = redraw(&mut app, &mut renderer);
    let original = app.project.settings.bounds;
    let ratio =
        original.width() / f64::from(old.width) / (original.height() / f64::from(old.height));
    let _ = app.update(Message::ToggleEditor);
    let full = redraw(&mut app, &mut renderer);
    assert!(full.width > old.width * 1.4);
    let bounds = app.project.settings.bounds;
    let after = bounds.width() / f64::from(full.width) / (bounds.height() / f64::from(full.height));
    assert!((ratio - after).abs() < 1e-12);
    let _ = app.update(Message::ToggleEditor);
    let restored = redraw(&mut app, &mut renderer);
    assert_eq!(old, restored);
    assert!((app.project.settings.bounds.x_min - original.x_min).abs() < 1e-10);
    assert!(!app.history.can_undo());
}

#[test]
fn collapsed_path_cards_keep_only_header_fields_and_can_stay_open_together() {
    let mut app = completed_app(Mode::Complex);
    app.busy = true;
    let _ = app.update(Message::AddPath(model::InputKind::Interval));
    let _ = app.update(Message::AddPath(model::InputKind::Point));
    app.tab = Tab::Path;
    let mut renderer = software_renderer();
    let count = |app: &App, renderer: &mut iced::Renderer| {
        let mut ui = UserInterface::build(
            app.view(),
            Size::new(1520., 960.),
            Default::default(),
            renderer,
        );
        let mut probe = FocusProbe::default();
        ui.operate(renderer, &mut probe);
        probe.fields.len()
    };
    // Each name stays editable. Curve: samples, extra and four Re/Im pairs.
    // Interval: start, end, samples, extra. Point: Re/Im.
    assert_eq!(count(&app, &mut renderer), 19);
    let _ = app.update(Message::TogglePath(0));
    assert_eq!(count(&app, &mut renderer), 9);
    let _ = app.update(Message::TogglePath(1));
    assert_eq!(count(&app, &mut renderer), 5);
    let _ = app.update(Message::TogglePath(2));
    assert_eq!(count(&app, &mut renderer), 3);
    let _ = app.update(Message::TogglePath(1));
    let _ = app.update(Message::TogglePath(2));
    assert_eq!(count(&app, &mut renderer), 9);
}

#[test]
fn dragging_rows_shows_space_before_release_commits_once_and_keeps_focus_identity() {
    drag_row_scene(false, false);
}

#[test]
fn edge_scrolling_follows_a_held_row_without_additional_mouse_motion() {
    use iced::{
        Event, Point,
        advanced::{shell, widget::operation::Outcome},
    };
    use smol::stream::StreamExt;
    fn send(
        app: &mut App,
        cache: &mut iced_runtime::user_interface::Cache,
        renderer: &mut iced::Renderer,
        event: Event,
        p: Point,
    ) {
        let size = Size::new(1100., 700.);
        let mut ui = UserInterface::build(app.view(), size, std::mem::take(cache), renderer);
        let mut bus = shell::Bus::new();
        ui.update(
            &iced::window::Headless,
            &shell::Waker::noop(),
            &[event],
            mouse::Cursor::Available(p),
            renderer,
            &mut bus,
        );
        ui.draw(
            renderer,
            &crate::theme(),
            &iced::advanced::renderer::Style {
                text_color: iced::Color::WHITE,
            },
            mouse::Cursor::Available(p),
        );
        *cache = ui.into_cache();
        for (message, _) in bus.drain() {
            if let Some(mut stream) = iced_runtime::task::into_stream(app.update(message)) {
                smol::block_on(async {
                    while let Some(action) = stream.next().await {
                        let iced_runtime::Action::Widget(mut op) = action else {
                            panic!("unexpected background task during drag")
                        };
                        let mut ui =
                            UserInterface::build(app.view(), size, std::mem::take(cache), renderer);
                        loop {
                            ui.operate(renderer, op.as_mut());
                            match op.finish() {
                                Outcome::Chain(next) => op = next,
                                _ => break,
                            }
                        }
                        drop(op);
                        *cache = ui.into_cache();
                    }
                });
            }
        }
    }
    let mut app = completed_app(Mode::Complex);
    app.busy = true;
    for _ in 0..12 {
        let _ = app.update(Message::Add);
    }
    let original = app.project.ordered_entries();
    let mut renderer = software_renderer();
    let mut ui = UserInterface::build(
        app.view(),
        Size::new(1100., 700.),
        Default::default(),
        &mut renderer,
    );
    let mut rows = RowBounds::default();
    ui.operate(&renderer, &mut rows);
    let mut probe = FocusProbe::default();
    ui.operate(&renderer, &mut probe);
    let viewport = probe.scroll[0].0;
    let first = rows.0[&0];
    let start = Point::new(first.x + 20., first.y + 20.);
    let bottom = Point::new(first.x + 80., viewport.y + viewport.height + 12.);
    let mut cache = ui.into_cache();
    send(
        &mut app,
        &mut cache,
        &mut renderer,
        Event::Mouse(mouse::Event::CursorMoved { position: start }),
        start,
    );
    send(
        &mut app,
        &mut cache,
        &mut renderer,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        start,
    );
    assert!(app.entry_drag.is_some());
    send(
        &mut app,
        &mut cache,
        &mut renderer,
        Event::Mouse(mouse::Event::CursorMoved { position: bottom }),
        bottom,
    );
    let now = std::time::Instant::now();
    for i in 0..150 {
        send(
            &mut app,
            &mut cache,
            &mut renderer,
            Event::Window(iced::window::Event::RedrawRequested(
                now + std::time::Duration::from_millis(i * 16),
            )),
            bottom,
        );
    }
    let mut ui = UserInterface::build(app.view(), Size::new(1100., 700.), cache, &mut renderer);
    let mut probe = FocusProbe::default();
    ui.operate(&renderer, &mut probe);
    assert!(
        probe.scroll[0].1.y > 1250. && probe.scroll[0].1.y < 1800.,
        "holding near the edge must scroll: {:?}",
        probe.scroll
    );
    assert!(app.entry_drag.is_some());
    assert_eq!(app.project.ordered_entries(), original);
    let down = probe.scroll[0].1.y;
    let mut cache = ui.into_cache();
    let top = Point::new(bottom.x, viewport.y - 12.);
    send(
        &mut app,
        &mut cache,
        &mut renderer,
        Event::Mouse(mouse::Event::CursorMoved { position: top }),
        top,
    );
    for i in 150..210 {
        send(
            &mut app,
            &mut cache,
            &mut renderer,
            Event::Window(iced::window::Event::RedrawRequested(
                now + std::time::Duration::from_millis(i * 16),
            )),
            top,
        );
    }
    let mut ui = UserInterface::build(app.view(), Size::new(1100., 700.), cache, &mut renderer);
    let mut probe = FocusProbe::default();
    ui.operate(&renderer, &mut probe);
    assert!(
        probe.scroll[0].1.y < down - 500.,
        "holding above the viewport must also scroll upward"
    );
    assert_eq!(
        app.project.ordered_entries(),
        original,
        "scrolling alone must not commit the reorder"
    );
    let mut cache = ui.into_cache();
    // Return into the actual drop area; edge overshoot must not enlarge it.
    let drop = Point::new(top.x, viewport.y + 8.);
    send(
        &mut app,
        &mut cache,
        &mut renderer,
        Event::Mouse(mouse::Event::CursorMoved { position: drop }),
        drop,
    );
    send(
        &mut app,
        &mut cache,
        &mut renderer,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        drop,
    );
    assert!(app.entry_drag.is_none());
    assert!(
        app.project
            .ordered_entries()
            .iter()
            .position(|&id| id == 0)
            .unwrap()
            >= 2
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.ordered_entries(), original);
}

#[test]
fn row_drag_accepts_twenty_pixels_left_of_the_card_in_the_full_editor() {
    drag_row_scene(false, true);
}

#[test]
fn numeric_editor_accepts_newlines_joins_tab_order_and_leaves_history_shortcuts_global() {
    use iced::{
        Event,
        advanced::shell,
        keyboard::{
            self, Key,
            key::{Code, Named, Physical},
        },
    };
    let mut app = numeric_definition_app();
    let mut renderer = software_renderer();
    let mut ui = UserInterface::build(
        app.view(),
        Size::new(1100., 700.),
        Default::default(),
        &mut renderer,
    );
    for _ in 0..3 {
        apply_operation(
            &mut ui,
            &mut renderer,
            super::focus::navigation("input-editor", false),
        );
    }
    let mut probe = FocusProbe::default();
    ui.operate(&renderer, &mut probe);
    assert!(probe.fields.iter().filter(|f| f.2).any(|f| {
        f.0.as_ref()
            .is_some_and(|id| id.contains("numeric-definition-0"))
    }));
    let mut bus = shell::Bus::new();
    let key = |key: Key, code: Code, modifiers| {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: Physical::Code(code),
            location: keyboard::Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        })
    };
    ui.update(
        &iced::window::Headless,
        &shell::Waker::noop(),
        &[key(
            Key::Named(Named::Enter),
            Code::Enter,
            keyboard::Modifiers::default(),
        )],
        mouse::Cursor::Unavailable,
        &mut renderer,
        &mut bus,
    );
    let edits: Vec<_> = bus
        .drain()
        .map(|(m, _)| m)
        .filter(|m| matches!(m, Message::EditNumeric(..)))
        .collect();
    assert_eq!(edits.len(), 1);
    assert!(matches!(
        &edits[0],
        Message::EditNumeric(
            0,
            iced::widget::text_editor::Action::Edit(iced::widget::text_editor::Edit::Enter)
        )
    ));
    for event in [
        key(
            Key::Named(Named::Tab),
            Code::Tab,
            keyboard::Modifiers::default(),
        ),
        key(
            Key::Character("z".into()),
            Code::KeyZ,
            keyboard::Modifiers::CTRL,
        ),
    ] {
        ui.update(
            &iced::window::Headless,
            &shell::Waker::noop(),
            &[event],
            mouse::Cursor::Unavailable,
            &mut renderer,
            &mut bus,
        );
        assert!(
            !bus.drain()
                .any(|(m, _)| matches!(m, Message::EditNumeric(..))),
            "Tab and undo must not edit the body"
        );
    }
    apply_operation(
        &mut ui,
        &mut renderer,
        super::focus::navigation("input-editor", false),
    );
    let mut probe = FocusProbe::default();
    ui.operate(&renderer, &mut probe);
    assert_eq!(
        probe.fields.iter().position(|f| f.2),
        Some(3),
        "Tab advances to project path"
    );
    apply_operation(
        &mut ui,
        &mut renderer,
        super::focus::navigation("input-editor", true),
    );
    let mut probe = FocusProbe::default();
    ui.operate(&renderer, &mut probe);
    assert_eq!(
        probe.fields.iter().position(|f| f.2),
        Some(2),
        "Shift+Tab returns to code"
    );
    drop(ui);
    let original = app.project.clone();
    for message in edits {
        let _ = app.update(message);
    }
    assert_eq!(
        app.numeric_editors[&0].content.line_count(),
        original.globals[0].expression.lines().count() + 1
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.project, original);
}
