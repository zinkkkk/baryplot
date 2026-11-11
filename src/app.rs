use crate::{
    model::{self, Bounds, Compiled, Curve, Entry, Extent, Mode, Project},
    plot::{Interaction, Target},
    sampling::{self, Plan, RenderData},
};
use iced::{Task, widget::pane_grid};
use num_complex::Complex64 as C;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

pub(crate) mod appearance;
mod definitions;
mod entries;
mod fit_view;
mod focus;
mod history;
mod message;
mod numerical;
mod path_view;
mod paths;
#[cfg(test)]
mod presentation_tests;
#[cfg(test)]
mod preview;
mod projects;
mod render;
mod spectrum_view;
mod update;
use numerical::{FitAction, FitField, FitState, SpectrumState};
mod validation;
mod view;
use message::Pane;
pub use message::{Field, Flag, GlobalField, Message, Tab};
use render::{DomainKey, LineKey};
pub use render::{RenderResult, TextureUpload};
use validation::InputKey;
#[derive(Clone, Copy)]
struct EntryDrag {
    from: usize,
}
pub struct App {
    appearance_ui: appearance::Ui,
    history: history::History,
    hovered_entry: Option<usize>,
    last_hovered_entry: Option<usize>,
    front_entry: Option<usize>,
    entry_drag: Option<EntryDrag>,
    numeric_editors: std::collections::BTreeMap<usize, definitions::NumericEditor>,
    pub project: Project,
    panes: pane_grid::State<Pane>,
    tab: Tab,
    compiled: Option<Compiled>,
    points: Vec<model::Node>,
    issues: std::collections::BTreeMap<InputKey, String>,
    plan: Option<Plan>,
    data: Option<Arc<RenderData>>,
    error: Option<String>,
    axis: [String; 4],
    revision: u64,
    visual_revision: u64,
    // Explicit resets, resolution changes and invalid input cancel work; dragging does not.
    latest: Arc<AtomicU64>,
    busy: bool,
    dirty: bool,
    image_revision: u64,
    image_allocation: Option<iced::widget::image::Allocation>,
    domain_key: Option<DomainKey>,
    line_key: Option<LineKey>,
    domain_revision: u64,
    domain_pending: bool,
    line_pending: bool,
    draw_path: bool,
    path: String,
    notice: String,
    control_drafts: Vec<[String; 2]>,
    active_path: usize,
    collapsed_paths: std::collections::BTreeSet<usize>,
    plot_size: Option<iced::Size>,
    fit: FitState,
    spectrum: SpectrumState,
}
impl App {
    pub fn new(mut project: Project) -> (Self, Task<Message>) {
        if project.settings.mode == Mode::Complex {
            project.split_components();
        }
        project.settings.resolve_path_kinds();
        let panes = pane_grid::State::with_configuration(pane_grid::Configuration::Split {
            axis: pane_grid::Axis::Vertical,
            ratio: 0.36,
            a: Box::new(pane_grid::Configuration::Pane(Pane::Editor)),
            b: Box::new(pane_grid::Configuration::Pane(Pane::Plot)),
        });
        let mut app = Self {
            axis: axis_strings(project.settings.bounds),
            control_drafts: control_strings(&project),
            active_path: 0,
            collapsed_paths: Default::default(),
            plot_size: None,
            project,
            numeric_editors: Default::default(),
            panes,
            tab: Tab::Nodes,
            compiled: None,
            points: Vec::new(),
            issues: std::collections::BTreeMap::new(),
            plan: None,
            data: None,
            error: None,
            revision: 0,
            visual_revision: 1,
            latest: Arc::new(AtomicU64::new(0)),
            busy: false,
            dirty: false,
            image_revision: 0,
            image_allocation: None,
            domain_key: None,
            line_key: None,
            domain_revision: 0,
            domain_pending: false,
            line_pending: false,
            draw_path: false,
            path: "baryplot.json".into(),
            notice: String::new(),
            appearance_ui: Default::default(),
            history: Default::default(),
            hovered_entry: None,
            last_hovered_entry: None,
            front_entry: None,
            entry_drag: None,
            fit: FitState::default(),
            spectrum: SpectrumState::default(),
        };
        app.rebuild();
        if !app.project.fit.function.is_empty() {
            app.tab = Tab::Fit;
        }
        let task = app.launch_all();
        (app, task)
    }
    fn foreground_entry(&self) -> Option<usize> {
        let count = self.displayed_project().entries.len();
        [
            self.hovered_entry,
            self.front_entry,
            self.last_hovered_entry,
        ]
        .into_iter()
        .flatten()
        .find(|&i| i < count)
    }
    pub fn subscription(&self) -> iced::Subscription<Message> {
        iced::event::listen_with(|event, _, _| match event {
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                match key.as_ref() {
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Tab)
                        if !modifiers.command() && !modifiers.alt() =>
                    {
                        Some(Message::NavigateFocus(modifiers.shift()))
                    }
                    iced::keyboard::Key::Character(c)
                        if modifiers.command() && c.eq_ignore_ascii_case("z") =>
                    {
                        Some(if modifiers.shift() {
                            Message::Redo
                        } else {
                            Message::Undo
                        })
                    }
                    iced::keyboard::Key::Character(c)
                        if modifiers.command() && c.eq_ignore_ascii_case("y") =>
                    {
                        Some(Message::Redo)
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) => {
                        Some(Message::Escape)
                    }
                    _ => None,
                }
            }
            iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                Some(Message::EndEntryDrag)
            }
            iced::Event::Window(iced::window::Event::Unfocused) => Some(Message::Escape),
            _ => None,
        })
    }
}
pub fn preset(mode: Mode) -> Project {
    let mut p = Project::default();
    p.settings.mode = mode;
    if mode == Mode::Complex {
        for (entry, (re, im)) in p.entries.iter_mut().zip([
            (["-2", "-0.5", "1"], ["-0.6", "1.2", "0"]),
            (["0.3", "1.3", "-0.7"], ["1.4", "0.2", "0.4"]),
            (["2", "0.4", "0.8"], ["-0.4", "-1", "-0.25"]),
        ]) {
            entry.expressions = re.map(str::to_owned);
            entry.imaginary = Some(im.map(str::to_owned));
        }
    }
    p
}

fn axis_strings(b: Bounds) -> [String; 4] {
    [b.x_min, b.x_max, b.y_min, b.y_max].map(|v| model::number(C::new(v, 0.)))
}
fn control_strings(p: &Project) -> Vec<[String; 2]> {
    p.settings.input.drafts()
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod performance;
