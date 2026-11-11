//! Bounded project history. Continuous input and plot gestures capture one before-state.
use super::*;
use std::{
    collections::{BTreeMap, VecDeque},
    time::{Duration, Instant},
};

#[derive(Clone)]
struct Snapshot {
    project: Project,
    axis: [String; 4],
    controls: Vec<[String; 2]>,
    active_path: usize,
    collapsed_paths: std::collections::BTreeSet<usize>,
    fit: Option<Arc<crate::numerics::aaa::Fit>>,
    preview: bool,
    stale: bool,
    plot_size: Option<iced::Size>,
    tab: Tab,
    drafts: BTreeMap<appearance::Number, String>,
    errors: BTreeMap<appearance::Number, String>,
}
impl Snapshot {
    fn new(app: &App) -> Self {
        Self {
            project: app.project.clone(),
            axis: app.axis.clone(),
            controls: app.control_drafts.clone(),
            active_path: app.active_path,
            collapsed_paths: app.collapsed_paths.clone(),
            fit: app.fit.result.clone(),
            preview: app.fit.preview,
            stale: app.fit.stale,
            plot_size: app.plot_size,
            tab: app.tab,
            drafts: app.appearance_ui.drafts.clone(),
            errors: app.appearance_ui.errors.clone(),
        }
    }
    fn unchanged(&self, app: &App) -> bool {
        self.project == app.project
            && self.axis == app.axis
            && self.controls == app.control_drafts
            && self.preview == app.fit.preview
            && same_fit(&self.fit, &app.fit.result)
            && self.drafts == app.appearance_ui.drafts
            && self.errors == app.appearance_ui.errors
    }
    fn bytes(&self) -> usize {
        let entries = self
            .project
            .entries
            .iter()
            .map(|e| {
                e.name.len()
                    + e.color.len()
                    + e.expressions.iter().map(String::len).sum::<usize>()
                    + e.imaginary
                        .as_ref()
                        .map_or(0, |v| v.iter().map(String::len).sum())
                    + 256
            })
            .sum::<usize>();
        let globals = self
            .project
            .globals
            .iter()
            .map(|g| g.name.len() + g.expression.len() + g.parameters.len() + 128)
            .sum::<usize>();
        let paths = self
            .project
            .settings
            .paths()
            .map(|p| {
                p.drafts()
                    .iter()
                    .flatten()
                    .map(|s| s.len() + 32)
                    .sum::<usize>()
                    + p.control_points.len() * 32
                    + 512
            })
            .sum::<usize>();
        entries
            + globals
            + paths
            + self
                .fit
                .as_ref()
                .map_or(0, |f| f.reference.len() * 64 + f.nodes.len() * 128)
            + 1024
    }
}
fn same_fit(
    a: &Option<Arc<crate::numerics::aaa::Fit>>,
    b: &Option<Arc<crate::numerics::aaa::Fit>>,
) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum GestureKind {
    Plot,
    Colour,
}
struct Gesture {
    kind: GestureKind,
    recorded: bool,
    redo: Vec<Snapshot>,
}
#[derive(Default)]
pub(super) struct History {
    undo: VecDeque<Snapshot>,
    redo: Vec<Snapshot>,
    last: Option<(String, Instant)>,
    gesture: Option<Gesture>,
}
impl History {
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    fn begin(&mut self, kind: GestureKind) {
        self.finish();
        self.gesture = Some(Gesture {
            kind,
            recorded: false,
            redo: Vec::new(),
        });
    }
    fn needs(&self, key: &Option<String>) -> bool {
        if let Some(g) = &self.gesture {
            return !g.recorded;
        }
        !matches!((&self.last,key),(Some((old,time)),Some(key)) if old==key&&time.elapsed()<Duration::from_millis(600))
    }
    fn record(&mut self, before: Snapshot, key: Option<String>) {
        self.undo.push_back(before);
        if let Some(g) = &mut self.gesture {
            g.recorded = true;
            g.redo = std::mem::take(&mut self.redo);
        } else {
            self.redo.clear();
            self.trim();
        }
        self.touch(key);
    }
    fn touch(&mut self, key: Option<String>) {
        self.last = key.map(|k| (k, Instant::now()));
    }
    fn trim(&mut self) {
        let mut bytes = self.undo.iter().map(Snapshot::bytes).sum::<usize>();
        while self.undo.len() > 1 && (self.undo.len() > 128 || bytes > 32 * 1024 * 1024) {
            bytes = bytes.saturating_sub(self.undo.pop_front().unwrap().bytes());
        }
    }
    fn finish(&mut self) {
        self.gesture = None;
        self.last = None;
        self.trim();
    }
    fn cancel_colour(&mut self) {
        if let Some(g) = self.gesture.take()
            && g.recorded
        {
            self.undo.pop_back();
            self.redo = g.redo;
        }
        self.last = None;
    }
}
impl App {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Undo => return self.restore_history(false),
            Message::Redo => return self.restore_history(true),
            Message::Plot(Interaction::BeginGesture) => {
                self.history.begin(GestureKind::Plot);
                return Task::none();
            }
            Message::Plot(Interaction::Finish(last)) => {
                let task = last.map_or_else(Task::none, |last| self.update(Message::Plot(*last)));
                self.history.finish();
                return task;
            }
            _ => {}
        }
        let colour_begin = matches!(&message, Message::Appearance(appearance::Action::Pick(_)));
        let colour_cancel = matches!(
            &message,
            Message::Appearance(appearance::Action::PickerCancel)
        ) || matches!(&message, Message::Escape)
            && self.appearance_ui.picker.is_some();
        let colour_accept = matches!(
            &message,
            Message::Appearance(appearance::Action::PickerAccept)
        );
        if colour_begin {
            self.history.begin(GestureKind::Colour);
        }
        if matches!(
            &message,
            Message::NavigateFocus(_)
                | Message::Tab(_)
                | Message::SelectPath(_)
                | Message::TogglePath(_)
        ) {
            self.history.last = None;
        }
        let key = edit_key(&message, self);
        let before = key
            .as_ref()
            .filter(|key| self.history.needs(key))
            .map(|_| Snapshot::new(self));
        let task = self.update_inner(message.clone());
        if let Some(before) = before {
            if !before.unchanged(self) {
                self.history.record(before, key.flatten());
            }
        } else if let Some(key) = key {
            self.history.touch(key);
        }
        if colour_cancel {
            self.history.cancel_colour();
        } else if colour_accept {
            self.history.finish();
        }
        if matches!(message, Message::EndEntryDrag) {
            self.history.last = None;
        }
        if matches!(message, Message::Escape)
            && self
                .history
                .gesture
                .as_ref()
                .is_some_and(|g| g.kind == GestureKind::Plot)
        {
            self.history.finish();
        }
        task
    }
    fn restore_history(&mut self, redo: bool) -> Task<Message> {
        // A live picker is one edit, and undo accepts its current preview before reverting it.
        self.appearance_ui.picker = None;
        self.history.finish();
        let next = if redo {
            self.history.redo.pop()
        } else {
            self.history.undo.pop_back()
        };
        let Some(mut next) = next else {
            return Task::none();
        };
        let current = Snapshot::new(self);
        if redo {
            self.history.undo.push_back(current);
        } else {
            self.history.redo.push(current);
        }
        let changed_fit_inputs = self.project.globals != next.project.globals
            || self.project.fit != next.project.fit
            || self.project.settings.mode != next.project.settings.mode
            || self
                .project
                .entries
                .iter()
                .map(|e| (&e.expressions, &e.imaginary, e.locks, e.enabled))
                .ne(next
                    .project
                    .entries
                    .iter()
                    .map(|e| (&e.expressions, &e.imaginary, e.locks, e.enabled)))
            || !same_fit(&self.fit.result, &next.fit);
        if let (Some(old), Some(size)) = (next.plot_size, self.plot_size) {
            let b = crate::axes::resize_view(
                next.project.settings.bounds,
                next.project.settings.guides.mapping,
                old,
                size,
            );
            if b != next.project.settings.bounds {
                next.project.settings.bounds = b;
                next.axis = axis_strings(b);
            }
        }
        self.project = next.project;
        self.axis = next.axis;
        self.active_path = next
            .active_path
            .min(self.project.settings.additional_paths.len());
        self.collapsed_paths = next.collapsed_paths;
        self.control_drafts = next.controls;
        self.tab = next.tab;
        if changed_fit_inputs {
            self.invalidate_fit();
        }
        self.fit.result = next.fit;
        self.fit.preview = next.preview && self.fit.result.is_some();
        self.fit.stale = next.stale;
        self.appearance_ui.drafts = next.drafts;
        self.appearance_ui.errors = next.errors;
        self.clear_entry_selection();
        self.draw_path = false;
        self.rebuild();
        self.history.trim();
        self.launch_all()
    }
}
// None: presentation/asynchronous bookkeeping. Some(None): one discrete edit.
// Some(Some(key)): coalesce continuous edits to the same field.
fn edit_key(message: &Message, app: &App) -> Option<Option<String>> {
    edit_key_for_path(message, app, app.active_path)
}
fn edit_key_for_path(message: &Message, app: &App, path: usize) -> Option<Option<String>> {
    use Message::*;
    let key = match message {
        EditPath(index, edit) => return edit_key_for_path(edit, app, *index),
        Edit(i, j, _) => Some(format!("parameter:{i}:{j}:re")),
        EditImaginary(i, j, _) => Some(format!("parameter:{i}:{j}:im")),
        Name(i, _) => Some(format!("name:{i}")),
        GlobalEdit(i, f, _) => Some(format!("global:{i}:{f:?}")),
        EditNumeric(i, action) if action.is_edit() => Some(format!("global:{i}:Expression")),
        EditNumeric(..) => return None,
        Field(f, _) => Some(format!("field:{}:{f:?}", path)),
        Axis(i, _) => Some(format!("axis:{i}")),
        ControlAxis(i, j, _) => Some(format!("control:{}:{i}:{j}", path)),
        ControlWeight(i, _) => Some(format!("weight:{}:{i}", path)),
        PathName(_) => Some(format!("path-name:{}", path)),
        LineOpacity(_) => Some("line-opacity".into()),
        ColorOpacity(_) => Some("colour-opacity".into()),
        Plot(Interaction::Move(target, _)) => Some(format!("move:{target:?}")),
        Plot(Interaction::Bounds(_)) => Some("view".into()),
        Appearance(a) => match a {
            appearance::Action::Open(_)
            | appearance::Action::Close
            | appearance::Action::Pick(_)
            | appearance::Action::PickerAccept
            | appearance::Action::PickerCancel => return None,
            appearance::Action::Number(field, _) => Some(format!("appearance-number:{field:?}")),
            appearance::Action::Width(line, _) => Some(format!("line-width:{line:?}")),
            appearance::Action::Opacity(line, _) => Some(format!("line-opacity:{line:?}")),
            appearance::Action::Guide(g) => match g {
                appearance::guides::Action::StrokeWidth(guide, _) => {
                    Some(format!("guide-width:{guide:?}"))
                }
                appearance::guides::Action::StrokeOpacity(guide, _) => {
                    Some(format!("guide-opacity:{guide:?}"))
                }
                appearance::guides::Action::Ticks(x, _) => Some(format!("ticks:{x}")),
                appearance::guides::Action::Subdivisions(x, _) => Some(format!("subdivisions:{x}")),
                appearance::guides::Action::Stride(x, _) => Some(format!("label-stride:{x}")),
                appearance::guides::Action::Precision(_) => Some("label-precision".into()),
                _ => None,
            },
            appearance::Action::Saturation(_)
            | appearance::Action::ShadingFloor(_)
            | appearance::Action::BandFloor(_)
            | appearance::Action::BandStrength(_)
            | appearance::Action::Strength(_)
            | appearance::Action::ContourWidth(_)
            | appearance::Action::PickerEdit(_) => {
                Some(format!("appearance:{:?}", std::mem::discriminant(a)))
            }
            _ => None,
        },
        Fit(FitAction::Text(field, _)) => Some(format!("fit:{field:?}")),
        Spectrum(crate::numerics::filter::Action::Text(field, _)) => {
            Some(format!("spectrum:{field:?}"))
        }
        Fit(FitAction::Refit | FitAction::Cancel) => return None,
        Fitted(g, Ok(Some(_)))
            if app.fit.running_generation == Some(*g) && *g == app.fit.generation =>
        {
            None
        }
        Fitted(..) => return None,
        ReorderEntry(..) => None,
        Enabled(..)
        | Visible(..)
        | Lock(..)
        | Add
        | Remove(..)
        | ClearEntries
        | GlobalKind(..)
        | AddGlobal
        | RemoveGlobal(..)
        | Mode(..)
        | Flag(..)
        | Extent(..)
        | Curve(..)
        | AddPath(..)
        | RemovePath
        | PathEnabled(..)
        | PathKind(..)
        | ResetPath
        | AddControl
        | ClearPath
        | Plot(
            Interaction::AddControl(_)
            | Interaction::RemoveControl(_)
            | Interaction::RemovePathControl(..),
        )
        | Spectrum(..)
        | Fit(..)
        | ResetView
        | Load
        | Preset(..) => None,
        _ => return None,
    };
    Some(key)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod fit_tests;
