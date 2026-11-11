mod app;
mod appearance;
mod axes;
mod dialog;
mod expression;
mod model;
mod numerics;
mod picker;
mod plot;
mod reorder;
mod sampling;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--numerical-worker") {
        return numerics::worker::serve().map_err(Into::into);
    }
    let mut mode = model::Mode::Real;
    let mut path = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--complex" => mode = model::Mode::Complex,
            "--project" => path = Some(args.next().ok_or("--project requires a JSON path")?),
            "--help" | "-h" => {
                println!(
                    "Baryplot\n  --complex          Start with a complex example\n  --project PATH     Open a saved JSON project\n  --help             Show this help"
                );
                return Ok(());
            }
            _ => return Err(format!("Unknown argument: {arg}").into()),
        }
    }
    let project = if let Some(path) = path {
        let project: model::Project = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        let compiled = project.compile()?;
        sampling::plan(&project.settings, &compiled)?;
        project
    } else {
        app::preset(mode)
    };
    iced::application(
        move || app::App::new(project.clone()),
        app::App::update,
        app::App::view,
    )
    .title("Baryplot · Rational function workbench")
    .theme(theme())
    .subscription(app::App::subscription)
    .window(iced::window::Settings {
        size: iced::Size::new(1520., 960.),
        min_size: Some(iced::Size::new(1100., 700.)),
        ..Default::default()
    })
    .run()?;
    Ok(())
}

fn theme() -> iced::Theme {
    iced::Theme::custom(
        "Baryplot",
        iced::theme::palette::Seed {
            background: iced::Color::from_rgb(0.043, 0.059, 0.086),
            text: iced::Color::from_rgb(0.87, 0.91, 0.96),
            primary: iced::Color::from_rgb(0.38, 0.84, 0.76),
            success: iced::Color::from_rgb(0.38, 0.84, 0.76),
            warning: iced::Color::from_rgb(1., 0.72, 0.42),
            danger: iced::Color::from_rgb(1., 0.46, 0.59),
        },
    )
}
