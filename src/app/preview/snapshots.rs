//! Headless snapshots checks. Common renderer/scene helpers live in the parent.
use super::*;

#[test]
#[ignore = "writes UI preview PNGs; run explicitly when checking layout"]
fn render_previews() {
    let output =
        std::env::var("BARYPLOT_PREVIEW_DIR").unwrap_or_else(|_| "/tmp/baryplot-previews".into());
    std::fs::create_dir_all(&output).expect("preview directory");
    let mut renderer = software_renderer();
    for mode in [Mode::Real, Mode::Complex] {
        let mut app = completed_app(mode);
        app.project.globals.extend([
            model::Definition {
                kind: model::DefinitionKind::Function,
                name: "wave".into(),
                parameters: "x, phase".into(),
                expression: "2*sin(x+phase)".into(),
            },
            model::Definition {
                kind: model::DefinitionKind::Variable,
                name: "z0".into(),
                parameters: String::new(),
                expression: "2+3i".into(),
            },
            model::Definition {
                kind: model::DefinitionKind::Function,
                name: "turn".into(),
                parameters: "z, t".into(),
                expression: "z*cis(t)".into(),
            },
            model::Definition {
                kind: model::DefinitionKind::Function,
                name: "target".into(),
                parameters: "x".into(),
                expression: if mode == Mode::Real {
                    "exp(x)"
                } else {
                    "tanh(4*x)"
                }
                .into(),
            },
        ]);
        app.rebuild();
        for (tab, name) in [
            (Tab::Nodes, "nodes"),
            (Tab::Definitions, "definitions"),
            (Tab::Fit, "fit-empty"),
            (Tab::Plot, "plot"),
            (Tab::Path, "path"),
            (Tab::Help, "guide"),
        ] {
            app.tab = tab;
            let path = format!("{output}/{}-{name}.png", mode.to_string().to_lowercase());
            snapshot(&app, &mut renderer)
                .save_png(&path)
                .expect("save PNG");
            println!("{path}");
        }
        app.tab = Tab::Definitions;
        let path = format!(
            "{output}/{}-definitions-small.png",
            mode.to_string().to_lowercase()
        );
        snapshot_at(&app, &mut renderer, Size::new(1100, 700))
            .save_png(&path)
            .expect("small table preview");
        let _ = app.update(Message::GlobalEdit(
            0,
            GlobalField::Expression,
            "sin(".into(),
        ));
        let path = format!(
            "{output}/{}-definitions-invalid.png",
            mode.to_string().to_lowercase()
        );
        snapshot(&app, &mut renderer)
            .save_png(&path)
            .expect("invalid table preview");
        let _ = app.update(Message::GlobalEdit(
            0,
            GlobalField::Expression,
            "2*sin(x+phase)".into(),
        ));
        app.tab = Tab::Nodes;
        let _ = app.update(Message::Edit(0, 1, "sin(".into()));
        let path = format!("{output}/{}-invalid.png", mode.to_string().to_lowercase());
        snapshot(&app, &mut renderer)
            .save_png(&path)
            .expect("invalid-input preview");
        println!("{path}");
        // Complete fitting and spectral jobs explicitly for deterministic layout previews.
        app.project.entries = preset(mode).entries;
        app.project.fit.function = "target".into();
        let request = crate::numerics::aaa::Request::new(&app.project).expect("fit request");
        let fit = crate::numerics::aaa::fit(&request, 0, &AtomicU64::new(0), |_| {})
            .expect("fit")
            .expect("current fit");
        app.fit.result = Some(Arc::new(fit));
        app.fit.preview = true;
        app.fit.error = None;
        app.frame_fit();
        app.rebuild();
        app.project.settings.analysis = crate::numerics::AnalysisOptions {
            poles: true,
            zeros: true,
            residues: true,
        };
        app.rebuild();
        app.spectrum.result = Some(Arc::new(
            crate::numerics::analyze_with_reference(
                app.compiled.as_ref().expect("fit model"),
                app.project.settings.analysis,
                0,
                &AtomicU64::new(0),
                Some(&app.fit.result.as_ref().expect("fit").reference),
            )
            .expect("spectrum")
            .expect("current spectrum"),
        ));
        let data = sampling::generate(
            app.compiled.as_ref().expect("fit model"),
            &app.project.settings,
            app.plan.as_ref().expect("plan"),
            app.latest.load(Ordering::Relaxed),
            &app.latest,
            |_| {},
        )
        .expect("fit colour data");
        app.data = Some(Arc::new(data));
        app.busy = false;
        app.domain_pending = false;
        app.dirty = false;
        app.tab = Tab::Fit;
        let path = format!("{output}/{}-aaa.png", mode.to_string().to_lowercase());
        snapshot(&app, &mut renderer)
            .save_png(&path)
            .expect("AAA preview");
        let path = format!("{output}/{}-aaa-small.png", mode.to_string().to_lowercase());
        snapshot_at(&app, &mut renderer, Size::new(1100, 700))
            .save_png(&path)
            .expect("small AAA preview");
        app.tab = Tab::Spectrum;
        app.frame_spectrum();
        let data = sampling::generate(
            app.compiled.as_ref().expect("model"),
            &app.project.settings,
            app.plan.as_ref().expect("plan"),
            app.latest.load(Ordering::Relaxed),
            &app.latest,
            |_| {},
        )
        .expect("framed data");
        app.data = Some(Arc::new(data));
        app.busy = false;
        app.domain_pending = false;
        app.dirty = false;
        let path = format!("{output}/{}-spectrum.png", mode.to_string().to_lowercase());
        snapshot(&app, &mut renderer)
            .save_png(&path)
            .expect("spectrum preview");
        let path = format!(
            "{output}/{}-spectrum-small.png",
            mode.to_string().to_lowercase()
        );
        snapshot_at(&app, &mut renderer, Size::new(1100, 700))
            .save_png(&path)
            .expect("small spectrum preview");
    }

    let project: Project = serde_json::from_str(include_str!(
        "../../../examples/complex-zeros-real-mode.json"
    ))
    .expect("real colouring example");
    let (mut app, _) = App::new(project);
    app.project.settings.real_coloring = true;
    app.project.settings.unit_circle = true;
    app.project.settings.bounds = Bounds {
        x_min: -2.5,
        x_max: 2.5,
        y_min: -2.5,
        y_max: 3.5,
    };
    app.axis = axis_strings(app.project.settings.bounds);
    app.rebuild();
    let data = sampling::generate(
        app.compiled.as_ref().expect("model"),
        &app.project.settings,
        app.plan.as_ref().expect("plan"),
        app.latest.load(Ordering::Relaxed),
        &app.latest,
        |_| {},
    )
    .expect("real colour field");
    app.data = Some(Arc::new(data));
    app.spectrum.result = Some(Arc::new(
        crate::numerics::analyze(
            app.compiled.as_ref().expect("model"),
            app.project.settings.analysis,
            0,
            &AtomicU64::new(0),
        )
        .expect("spectrum")
        .expect("current"),
    ));
    app.spectrum.busy = false;
    app.spectrum.pending = None;
    app.busy = false;
    app.domain_pending = false;
    app.dirty = false;
    app.tab = Tab::Spectrum;
    snapshot(&app, &mut renderer)
        .save_png(format!("{output}/real-colour-unit-circle.png"))
        .expect("real colour and unit circle preview");
    snapshot_at(&app, &mut renderer, Size::new(1100, 700))
        .save_png(format!("{output}/real-colour-unit-circle-small.png"))
        .expect("small real colour and unit circle preview");
}

#[test]
#[ignore = "writes display controls and compact-card preview images"]
fn render_appearance_previews() {
    let output = "/tmp/baryplot-previews";
    std::fs::create_dir_all(output).expect("preview directory");
    let mut renderer = software_renderer();
    for mode in [Mode::Real, Mode::Complex] {
        let mut app = completed_app(mode);
        let name = mode.to_string().to_lowercase();
        for panel in [appearance::Panel::Lines, appearance::Panel::Colour] {
            let _ = app.update(Message::Appearance(appearance::Action::Open(panel)));
            let suffix = if panel == appearance::Panel::Lines {
                "line-controls"
            } else {
                "colour-controls"
            };
            snapshot_at(&app, &mut renderer, Size::new(1100, 700))
                .save_png(format!("{output}/{name}-{suffix}.png"))
                .expect("display dialog");
        }
        let _ = app.update(Message::Appearance(appearance::Action::Close));
        let _ = app.update(Message::Appearance(appearance::Action::Pick(
            crate::picker::Target::Entry(0),
        )));
        snapshot_at(&app, &mut renderer, Size::new(1100, 700))
            .save_png(format!("{output}/{name}-colour-picker.png"))
            .expect("colour picker");
        let _ = app.update(Message::Appearance(appearance::Action::PickerCancel));
        let _ = app.update(Message::BeginEntryDrag(2));
        let _ = app.update(Message::ReorderEntry(2, 0));
        let _ = app.update(Message::EndEntryDrag);
        snapshot_at(&app, &mut renderer, Size::new(1100, 700))
            .save_png(format!("{output}/{name}-compact-hover.png"))
            .expect("hovered reordered cards");
    }
}

#[test]
#[ignore = "writes guide, axis and input-path previews"]
fn render_guide_previews() {
    let output = "/tmp/baryplot-previews";
    std::fs::create_dir_all(output).unwrap();
    let mut renderer = software_renderer();
    let mut app = completed_app(Mode::Real);
    let _ = app.update(Message::Appearance(appearance::Action::Open(
        appearance::Panel::Guides,
    )));
    snapshot_at(&app, &mut renderer, Size::new(1100, 700))
        .save_png(format!("{output}/guides.png"))
        .unwrap();
    for (panel, name) in [
        (appearance::Panel::Circle, "circle-settings"),
        (appearance::Panel::Background, "background-settings"),
    ] {
        let _ = app.update(Message::Appearance(appearance::Action::Open(panel)));
        snapshot_at(&app, &mut renderer, Size::new(1100, 700))
            .save_png(format!("{output}/{name}.png"))
            .unwrap();
    }
    let _ = app.update(Message::Appearance(appearance::Action::Close));
    app.tab = Tab::Path;
    snapshot_at(&app, &mut renderer, Size::new(1100, 700))
        .save_png(format!("{output}/real-path.png"))
        .unwrap();
    app.project.settings.guides.background = [0.93, 0.94, 0.96];
    app.project.settings.guides.label_color = [0.2, 0.23, 0.3];
    app.project.settings.guides.major_style.color = [0.3, 0.4, 0.5];
    app.project.settings.guides.minor = true;
    app.project.settings.unit_circle = true;
    app.project.settings.guides.circle_style.dashed = true;
    app.project.settings.guides.mapping.x = crate::axes::Scale::SymLog;
    app.visual_revision += 1;
    snapshot_at(&app, &mut renderer, Size::new(1100, 700))
        .save_png(format!("{output}/light-guides.png"))
        .unwrap();
    let mut app = completed_app(Mode::Complex);
    app.tab = Tab::Path;
    snapshot_at(&app, &mut renderer, Size::new(1100, 700))
        .save_png(format!("{output}/complex-path.png"))
        .unwrap();
    app.project.settings.resolution = 512;
    let _ = app.update(Message::Appearance(appearance::Action::Guide(
        appearance::guides::Action::Scale(true, crate::axes::Scale::Log10),
    )));
    app.data = Some(Arc::new(
        sampling::generate(
            app.compiled.as_ref().unwrap(),
            &app.project.settings,
            app.plan.as_ref().unwrap(),
            app.latest.load(Ordering::Relaxed),
            &app.latest,
            |_| {},
        )
        .unwrap(),
    ));
    snapshot_at(&app, &mut renderer, Size::new(1100, 700))
        .save_png(format!("{output}/log-guides.png"))
        .unwrap();
    let mut original = completed_app(Mode::Complex);
    let _ = original.update(Message::Appearance(appearance::Action::Classic));
    original.data = Some(Arc::new(
        sampling::generate(
            original.compiled.as_ref().unwrap(),
            &original.project.settings,
            original.plan.as_ref().unwrap(),
            original.latest.load(Ordering::Relaxed),
            &original.latest,
            |_| {},
        )
        .unwrap(),
    ));
    snapshot_at(&original, &mut renderer, Size::new(1100, 700))
        .save_png(format!("{output}/original-colouring.png"))
        .unwrap();
    let _ = original.update(Message::Appearance(appearance::Action::Open(
        appearance::Panel::Colour,
    )));
    snapshot_at(&original, &mut renderer, Size::new(1100, 700))
        .save_png(format!("{output}/original-colouring-settings.png"))
        .unwrap();
}

#[test]
#[ignore = "writes expanded/collapsed input panel previews"]
fn render_collapsed_previews() {
    let output = "/tmp/baryplot-previews";
    std::fs::create_dir_all(output).unwrap();
    let mut renderer = software_renderer();
    for mode in [Mode::Real, Mode::Complex] {
        let mut app = completed_app(mode);
        let name = mode.to_string().to_lowercase();
        let expanded = snapshot_at(&app, &mut renderer, Size::new(1100, 700));
        expanded
            .save_png(format!("{output}/{name}-editor-expanded.png"))
            .unwrap();
        let _ = app.update(Message::ToggleEditor);
        snapshot_at(&app, &mut renderer, Size::new(1100, 700))
            .save_png(format!("{output}/{name}-editor-collapsed.png"))
            .unwrap();
        let _ = app.update(Message::ToggleEditor);
        let restored = snapshot_at(&app, &mut renderer, Size::new(1100, 700));
        assert!(
            restored.data() == expanded.data(),
            "restoring must recover the original pane layout"
        );
    }
}

#[test]
#[ignore = "writes cursor readout, interval handle and moved-dialog previews"]
fn render_interaction_previews() {
    use iced::{Event, Point, advanced::shell};
    let output = "/tmp/baryplot-previews";
    std::fs::create_dir_all(output).unwrap();
    let mut renderer = software_renderer();
    let size = Size::new(1100, 700);
    let logical = Size::new(1100., 700.);
    let mut app = completed_app(Mode::Real);
    app.tab = Tab::Path;
    app.project.settings.guides.cursor_color = [0.95, 0.95, 1.];
    let mut ui = UserInterface::build(app.view(), logical, Default::default(), &mut renderer);
    let cursor = mouse::Cursor::Available(Point::new(820., 460.));
    let mut bus = shell::Bus::new();
    ui.update(
        &iced::window::Headless,
        &shell::Waker::noop(),
        &[Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(820., 460.),
        })],
        cursor,
        &mut renderer,
        &mut bus,
    );
    paint_ui(&mut ui, &mut renderer, size, cursor)
        .save_png(format!("{output}/real-cursor-and-interval.png"))
        .unwrap();
    drop(ui);
    let mut app = completed_app(Mode::Complex);
    let _ = app.update(Message::Appearance(appearance::Action::Open(
        appearance::Panel::Colour,
    )));
    let mut ui = UserInterface::build(app.view(), logical, Default::default(), &mut renderer);
    for (event, p) in [
        (
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Point::new(350., 75.),
        ),
        (
            Event::Mouse(mouse::Event::CursorMoved {
                position: Point::new(100., 75.),
            }),
            Point::new(100., 75.),
        ),
        (
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            Point::new(100., 75.),
        ),
    ] {
        ui.update(
            &iced::window::Headless,
            &shell::Waker::noop(),
            &[event],
            mouse::Cursor::Available(p),
            &mut renderer,
            &mut bus,
        );
    }
    paint_ui(&mut ui, &mut renderer, size, mouse::Cursor::Unavailable)
        .save_png(format!("{output}/colour-dialog-moved.png"))
        .unwrap();
    let moved_parent = paint_ui(&mut ui, &mut renderer, size, mouse::Cursor::Unavailable);
    assert!(
        bus.is_empty(),
        "moving a title bar must not edit inputs or close the modal"
    );
    let cache = ui.into_cache();
    let _ = app.update(Message::Appearance(appearance::Action::Pick(
        crate::picker::Target::Line(crate::appearance::Line::Output),
    )));
    let mut ui = UserInterface::build(app.view(), logical, cache, &mut renderer);
    paint_ui(&mut ui, &mut renderer, size, mouse::Cursor::Unavailable)
        .save_png(format!("{output}/nested-dialogs.png"))
        .unwrap();
    for (event, p) in [
        (
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Point::new(420., 135.),
        ),
        (
            Event::Mouse(mouse::Event::CursorMoved {
                position: Point::new(720., 135.),
            }),
            Point::new(720., 135.),
        ),
        (
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            Point::new(720., 135.),
        ),
    ] {
        ui.update(
            &iced::window::Headless,
            &shell::Waker::noop(),
            &[event],
            mouse::Cursor::Available(p),
            &mut renderer,
            &mut bus,
        );
    }
    assert!(bus.is_empty(), "picker dragging must not change its colour");
    paint_ui(&mut ui, &mut renderer, size, mouse::Cursor::Unavailable)
        .save_png(format!("{output}/picker-dialog-moved.png"))
        .unwrap();
    // A moved colour picker must still deliver live colour edits from its body.
    let p = Point::new(900., 300.);
    for event in [
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ] {
        ui.update(
            &iced::window::Headless,
            &shell::Waker::noop(),
            &[event],
            mouse::Cursor::Available(p),
            &mut renderer,
            &mut bus,
        );
    }
    assert!(
        bus.drain()
            .any(|(m, _)| matches!(m, Message::Appearance(appearance::Action::PickerEdit(_)))),
        "moved picker controls must stay interactive"
    );
    let cache = ui.into_cache();
    let _ = app.update(Message::Appearance(appearance::Action::PickerCancel));
    let mut ui = UserInterface::build(app.view(), logical, cache, &mut renderer);
    let restored = paint_ui(&mut ui, &mut renderer, size, mouse::Cursor::Unavailable);
    assert!(
        restored.data() == moved_parent.data(),
        "nested picker must not reset the parent dialog position"
    );
}

#[test]
#[ignore = "writes fixed-header and multiple-input-path preview images"]
fn render_multi_path_previews() {
    let output = "/tmp/baryplot-previews";
    std::fs::create_dir_all(output).unwrap();
    let mut renderer = software_renderer();
    let mut app = completed_app(Mode::Complex);
    app.busy = true;
    let _ = app.update(Message::AddPath(model::InputKind::Interval));
    let _ = app.update(Message::Field(Field::Start, "-2.8".into()));
    let _ = app.update(Message::Field(Field::End, "2.8".into()));
    let _ = app.update(Message::AddPath(model::InputKind::Point));
    let _ = app.update(Message::ControlAxis(0, 0, "0.5".into()));
    let _ = app.update(Message::ControlAxis(0, 1, "0.9".into()));
    app.busy = false;
    app.tab = Tab::Path;
    snapshot_at(&app, &mut renderer, Size::new(1100, 700))
        .save_png(format!("{output}/multiple-input-paths.png"))
        .unwrap();
    let _ = app.update(Message::SelectPath(1));
    snapshot_at(&app, &mut renderer, Size::new(1100, 700))
        .save_png(format!("{output}/complex-real-interval.png"))
        .unwrap();
    app.tab = Tab::Nodes;
    for _ in 0..15 {
        let _ = app.update(Message::Add);
    }
    let mut ui = UserInterface::build(
        app.view(),
        Size::new(1100., 700.),
        Default::default(),
        &mut renderer,
    );
    apply_operation(
        &mut ui,
        &mut renderer,
        super::focus::navigation("input-editor", true),
    );
    paint_ui(
        &mut ui,
        &mut renderer,
        Size::new(1100, 700),
        mouse::Cursor::Unavailable,
    )
    .save_png(format!("{output}/sticky-add-row.png"))
    .unwrap();
}

#[test]
#[ignore = "writes independently collapsible input-path list previews"]
fn render_path_card_previews() {
    let output = "/tmp/baryplot-previews";
    std::fs::create_dir_all(output).unwrap();
    let mut app = completed_app(Mode::Complex);
    app.busy = true;
    let _ = app.update(Message::AddPath(model::InputKind::Interval));
    let _ = app.update(Message::AddPath(model::InputKind::Point));
    let _ = app.update(Message::ControlAxis(0, 0, "0.5".into()));
    let _ = app.update(Message::ControlAxis(0, 1, "0.9".into()));
    let _ = app.update(Message::TogglePath(0));
    app.busy = false;
    app.tab = Tab::Path;
    let mut renderer = software_renderer();
    snapshot(&app, &mut renderer)
        .save_png(format!("{output}/input-path-cards.png"))
        .unwrap();
    let _ = app.update(Message::TogglePath(1));
    snapshot_at(&app, &mut renderer, Size::new(1100, 700))
        .save_png(format!("{output}/input-path-cards-compact.png"))
        .unwrap();
}

#[test]
#[ignore = "writes actual widget drag-animation previews"]
fn render_row_drag_previews() {
    std::fs::create_dir_all("/tmp/baryplot-previews").unwrap();
    drag_row_scene(true, false);
}

#[test]
#[ignore = "writes NL-AAA controls and convergence previews at minimum window size"]
fn render_nonlinear_previews() {
    let output = "/tmp/baryplot-previews";
    std::fs::create_dir_all(output).unwrap();
    let mut app = completed_app(Mode::Real);
    app.project.globals = "target(x)=exp(x)".into();
    app.project.fit.function = "target".into();
    app.project.fit.method = crate::numerics::aaa::Method::Nonlinear;
    app.project.fit.lawson = "3".into();
    app.project.fit.lawson_damping = "0.4".into();
    let request = crate::numerics::aaa::Request::new(&app.project).unwrap();
    let result = crate::numerics::aaa::fit(&request, 0, &AtomicU64::new(0), |_| {})
        .unwrap()
        .unwrap();
    app.fit.result = Some(Arc::new(result));
    app.fit.preview = true;
    app.rebuild();
    app.data = Some(Arc::new(
        sampling::generate(
            app.compiled.as_ref().unwrap(),
            &app.project.settings,
            app.plan.as_ref().unwrap(),
            app.latest.load(Ordering::Relaxed),
            &app.latest,
            |_| {},
        )
        .unwrap(),
    ));
    app.tab = Tab::Fit;
    let mut renderer = software_renderer();
    snapshot_at(&app, &mut renderer, Size::new(1100, 700))
        .save_png(format!("{output}/aaa-nonlinear-controls.png"))
        .unwrap();
    let mut ui = UserInterface::build(
        app.view(),
        Size::new(1100., 700.),
        Default::default(),
        &mut renderer,
    );
    apply_operation(
        &mut ui,
        &mut renderer,
        super::focus::navigation("input-editor", true),
    );
    paint_ui(
        &mut ui,
        &mut renderer,
        Size::new(1100, 700),
        mouse::Cursor::Unavailable,
    )
    .save_png(format!("{output}/aaa-nonlinear-damping.png"))
    .unwrap();
    use iced::advanced::widget::{
        Id,
        operation::{Animation, scrollable},
    };
    apply_operation(
        &mut ui,
        &mut renderer,
        scrollable::snap_to(
            Id::from("editor-scroll"),
            scrollable::RelativeOffset::END.into(),
            Animation::Instant,
        ),
    );
    paint_ui(
        &mut ui,
        &mut renderer,
        Size::new(1100, 700),
        mouse::Cursor::Unavailable,
    )
    .save_png(format!("{output}/aaa-nonlinear-results.png"))
    .unwrap();
}

#[test]
#[ignore = "writes multiline numeric definition editor previews"]
fn render_numeric_definition_previews() {
    let mut app = numeric_definition_app();
    app.busy = false;
    app.dirty = false;
    app.line_pending = false;
    app.domain_pending = false;
    let mut renderer = software_renderer();
    std::fs::create_dir_all("/tmp/baryplot-previews").unwrap();
    snapshot_at(&app, &mut renderer, Size::new(1100, 700))
        .save_png("/tmp/baryplot-previews/numeric-definitions.png")
        .unwrap();
    let _ = app.update(Message::GlobalEdit(
        0,
        GlobalField::Expression,
        "let total = 0;\nfor k in 1..=12 {\n    total += ;\n}\ntotal".into(),
    ));
    snapshot_at(&app, &mut renderer, Size::new(1100, 700))
        .save_png("/tmp/baryplot-previews/numeric-definitions-invalid.png")
        .unwrap();
}

#[test]
#[ignore = "writes the README and guide screenshots after initializing widget styles"]
fn render_readme_previews() {
    let output = "/tmp/baryplot-readme-previews";
    std::fs::create_dir_all(output).unwrap();
    let mut renderer = software_renderer();
    for (name, mut app) in [
        ("real", completed_app(Mode::Real)),
        ("complex", completed_app(Mode::Complex)),
        ("numeric-functions", numeric_definition_app()),
    ] {
        app.busy = false;
        app.dirty = false;
        app.line_pending = false;
        app.domain_pending = false;
        let size = Size::new(1520, 960);
        let mut ui = UserInterface::build(
            app.view(),
            Size::new(1520., 960.),
            Default::default(),
            &mut renderer,
        );
        ui.update(
            &iced::window::Headless,
            &iced::advanced::shell::Waker::noop(),
            &[iced::Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            ))],
            mouse::Cursor::Unavailable,
            &mut renderer,
            &mut iced::advanced::shell::Bus::new(),
        );
        paint_ui(&mut ui, &mut renderer, size, mouse::Cursor::Unavailable)
            .save_png(format!("{output}/{name}.png"))
            .unwrap();
    }
}
