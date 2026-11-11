//! Independent fit and spectral workers. Neither waits on the UI or occupies the renderer pool.
use super::*;
use crate::numerics::{self, AnalysisOptions, Spectrum, aaa, worker};
#[derive(Debug, Clone, Copy)]
pub enum FitField {
    Start,
    End,
    Tolerance,
    MaxNodes,
    Degree,
    Lawson,
    LawsonDamping,
    SkSteps,
    WfSteps,
    RefinementTolerance,
}
#[derive(Debug, Clone)]
pub enum FitAction {
    Select(String),
    Method(aaa::Method),
    Text(FitField, String),
    Fixed(bool),
    Sign(bool),
    Cleanup(bool),
    Density(u32),
    Preview(bool),
    Apply,
    Frame,
    Refit,
    Cancel,
}
#[derive(Default)]
pub struct FitState {
    pub process: Option<worker::Job>,
    pub cancelled: bool,
    pub busy: bool,
    pub running_generation: Option<u64>,
    pub generation: u64,
    pub latest: Arc<AtomicU64>,
    pub key: Option<String>,
    pub pending: Option<(u64, Box<Project>)>,
    pub result: Option<Arc<aaa::Fit>>,
    pub progress: Option<aaa::Progress>,
    pub history: Vec<aaa::Step>,
    pub stale: bool,
    pub error: Option<String>,
    pub preview: bool,
    pub display_project: Option<Project>,
}
type SpectrumKey = (Vec<[C; 3]>, AnalysisOptions);
#[derive(Default)]
pub struct SpectrumState {
    pub process: Option<worker::Job>,
    pub running_generation: Option<u64>,
    pub paused: bool,
    pub details_requested: bool,
    pub busy: bool,
    pub generation: u64,
    pub latest: Arc<AtomicU64>,
    pub key: Option<SpectrumKey>,
    pub pending: Option<(u64, Compiled, AnalysisOptions)>,
    pub result: Option<Arc<Spectrum>>,
    pub error: Option<String>,
    pub filter: numerics::filter::DisplayFilter,
    pub available: AnalysisOptions,
    pub running_options: AnalysisOptions,
}
impl App {
    pub(super) fn displayed_project(&self) -> &Project {
        self.fit
            .display_project
            .as_ref()
            .filter(|_| self.fit.preview)
            .unwrap_or(&self.project)
    }
    fn stop_fit_process(&mut self) {
        self.fit.process = None;
        self.fit.busy = false;
        self.fit.running_generation = None;
    }
    fn stop_spectrum_process(&mut self) {
        self.spectrum.process = None;
        self.spectrum.busy = false;
        self.spectrum.running_generation = None;
        self.spectrum.pending = None;
        self.spectrum.generation += 1;
        self.spectrum
            .latest
            .store(self.spectrum.generation, Ordering::Relaxed);
    }
    pub(super) fn cancel_spectrum(&mut self) {
        self.stop_spectrum_process();
        self.spectrum.paused = true;
        self.spectrum.details_requested = false;
    }
    pub(super) fn reset_numerical(&mut self) {
        self.stop_fit_process();
        self.stop_spectrum_process();
        self.fit.cancelled = false;
        self.spectrum.paused = false;
        self.spectrum.details_requested = false;
        self.fit.generation += 1;
        self.fit
            .latest
            .store(self.fit.generation, Ordering::Relaxed);
        self.fit.pending = None;
        self.fit.key = None;
        self.fit.result = None;
        self.fit.progress = None;
        self.fit.history.clear();
        self.fit.stale = false;
        self.fit.error = None;
        self.fit.preview = false;
        self.fit.display_project = None;
        self.spectrum.generation += 1;
        self.spectrum
            .latest
            .store(self.spectrum.generation, Ordering::Relaxed);
        self.spectrum.key = None;
        self.spectrum.pending = None;
        self.spectrum.result = None;
        self.spectrum.available = AnalysisOptions::default();
        self.spectrum.error = None;
    }
    pub(super) fn invalidate_fit(&mut self) {
        self.stop_fit_process();
        self.fit.cancelled = false;
        self.fit.generation += 1;
        self.fit
            .latest
            .store(self.fit.generation, Ordering::Relaxed);
        self.fit.pending = None;
        self.fit.key = None;
        self.fit.progress = None;
        self.fit.history.clear();
        self.fit.error = None;
        self.fit.stale = !self.project.fit.function.is_empty();
    }
    pub(super) fn leave_fit_preview(&mut self) {
        self.fit.preview = false;
        self.invalidate_fit();
    }
    pub(super) fn preview_model(&mut self) -> Result<Compiled, String> {
        if self.fit.preview
            && let Some(fit) = &self.fit.result
        {
            let variables = self.project.resolve_definitions().variables;
            let m = Compiled::from_nodes(fit.nodes.clone(), variables)?;
            let mut display = self.project.clone();
            display.entries = fit_entries(&m.nodes);
            self.fit.display_project = Some(display);
            return Ok(m);
        }
        self.fit.display_project = None;
        self.project.compile()
    }
    pub(super) fn queue_fit(&mut self, force: bool) {
        if self.project.fit.function.is_empty() {
            return;
        }
        let key = serde_json::to_string(&(
            &self.project.fit,
            &self.project.globals,
            &self.project.entries,
            self.project.settings.mode,
        ))
        .expect("serializable fit inputs");
        if !force && self.fit.key.as_ref() == Some(&key) {
            return;
        }
        self.stop_fit_process();
        if self.spectrum.busy || self.spectrum.pending.is_some() {
            self.stop_spectrum_process();
        }
        self.fit.cancelled = false;
        self.fit.key = Some(key);
        self.fit.generation += 1;
        self.fit
            .latest
            .store(self.fit.generation, Ordering::Relaxed);
        self.fit.progress = None;
        self.fit.history.clear();
        match aaa::Request::new(&self.project) {
            Ok(_) => {
                self.fit.pending = Some((self.fit.generation, Box::new(self.project.clone())));
                self.fit.error = None;
            }
            Err(e) => {
                self.fit.pending = None;
                self.fit.error = Some(e);
            }
        }
    }
    pub(super) fn queue_spectrum(&mut self) {
        use numerics::filter::{Cleanup, DisplayFilter};
        self.issues
            .retain(|key, _| !matches!(key, InputKey::SpectrumFilter(_)));
        let parsed = if let Some(m) = &self.compiled {
            DisplayFilter::parse(&self.project.settings.spectrum, &m.variables)
        } else {
            DisplayFilter::parse(
                &self.project.settings.spectrum,
                &self.project.resolve_definitions().variables,
            )
        };
        match parsed {
            Ok(filter) => self.spectrum.filter = filter,
            Err((field, e)) => {
                self.issues.insert(InputKey::SpectrumFilter(field), e);
            }
        }
        let mut options = self.project.settings.analysis;
        let requested =
            !self.spectrum.paused && (options.enabled() || self.spectrum.details_requested);
        if !requested {
            if self.spectrum.busy || self.spectrum.pending.is_some() {
                self.stop_spectrum_process();
            }
            return;
        }
        if self.spectrum.details_requested
            || self.project.settings.spectrum.cleanup != Cleanup::Off
            || options.residues
        {
            options = AnalysisOptions {
                poles: true,
                zeros: true,
                residues: true,
            };
        }
        let Some(model) = self.compiled.as_ref().filter(|_| self.error.is_none()) else {
            self.stop_spectrum_process();
            return;
        };
        let terms: Vec<_> = model
            .nodes
            .iter()
            .map(|n| [n.z, n.value, n.weight])
            .collect();
        let same = self
            .spectrum
            .key
            .as_ref()
            .is_some_and(|(old, _)| *old == terms);
        let covers = |a: AnalysisOptions, b: AnalysisOptions| {
            (!b.poles || a.poles) && (!b.zeros || a.zeros) && (!b.residues || a.residues)
        };
        if same && self.spectrum.result.is_some() && covers(self.spectrum.available, options) {
            if self.spectrum.busy || self.spectrum.pending.is_some() {
                self.stop_spectrum_process();
            }
            return;
        }
        if same
            && self
                .spectrum
                .key
                .as_ref()
                .is_some_and(|(_, old)| *old == options)
            && (self.spectrum.busy
                || self.spectrum.pending.is_some()
                || self.spectrum.error.is_some())
        {
            return;
        }
        let model = model.clone();
        self.stop_spectrum_process();
        self.spectrum.error = None;
        if !same {
            if self.spectrum.result.take().is_some() {
                self.visual_revision += 1;
            }
            self.spectrum.available = AnalysisOptions::default();
        }
        self.spectrum.key = Some((terms, options));
        self.spectrum.pending = Some((self.spectrum.generation, model, options));
    }
    pub(super) fn launch_all(&mut self) -> Task<Message> {
        if let Some(display) = &mut self.fit.display_project {
            display.settings = self.project.settings.clone();
        }
        self.queue_spectrum();
        let tasks = [self.launch(), self.launch_fit(), self.launch_spectrum()];
        if tasks.iter().all(|task| task.units() == 0) {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }
    fn launch_fit(&mut self) -> Task<Message> {
        if self.fit.busy {
            return Task::none();
        }
        let Some((generation, project)) = self.fit.pending.take() else {
            return Task::none();
        };
        match worker::Job::spawn(worker::Request::Fit(project), std::time::Duration::ZERO) {
            Ok((job, receiver)) => {
                self.fit.process = Some(job);
                self.fit.busy = true;
                self.fit.running_generation = Some(generation);
                Task::run(receiver, move |event| match event {
                    worker::Event::Progress(p) => Message::FitProgress(generation, p),
                    worker::Event::Fitted(result) => {
                        Message::Fitted(generation, result.map(|fit| Some(*fit)))
                    }
                    worker::Event::Failed(e) => Message::Fitted(generation, Err(e)),
                    worker::Event::Cancelled => Message::Fitted(generation, Ok(None)),
                    _ => Message::Fitted(generation, Err("Unexpected AAA worker response".into())),
                })
            }
            Err(e) => {
                self.fit.error = Some(e);
                Task::none()
            }
        }
    }
    fn launch_spectrum(&mut self) -> Task<Message> {
        // Rendering and texture upload always get scheduled first. A pending analysis is
        // one replaceable snapshot, not a queue of every pointer position.
        if self.spectrum.busy
            || self.busy
            || self.domain_pending
            || self.line_pending
            || (self.fit.preview && self.fit.busy)
        {
            return Task::none();
        }
        let Some((generation, model, options)) = self.spectrum.pending.take() else {
            return Task::none();
        };
        self.spectrum.running_options = options;
        let reference = self
            .fit
            .result
            .as_ref()
            .filter(|fit| {
                fit.nodes.len() == model.nodes.len()
                    && fit
                        .nodes
                        .iter()
                        .zip(&model.nodes)
                        .all(|(a, b)| a.z == b.z && a.value == b.value && a.weight == b.weight)
            })
            .map(|fit| Arc::clone(&fit.reference));
        let request = worker::Request::Spectrum {
            nodes: model.nodes,
            options,
            reference,
        };
        // Coalesce rapid edits before paying process startup / decomposition costs.
        match worker::Job::spawn(request, std::time::Duration::from_millis(80)) {
            Ok((job, receiver)) => {
                self.spectrum.process = Some(job);
                self.spectrum.busy = true;
                self.spectrum.running_generation = Some(generation);
                Task::run(receiver, move |event| match event {
                    worker::Event::Analyzed(result) => {
                        Message::Analyzed(generation, result.map(|spectrum| Some(*spectrum)))
                    }
                    worker::Event::Failed(e) => Message::Analyzed(generation, Err(e)),
                    worker::Event::Cancelled => Message::Analyzed(generation, Ok(None)),
                    _ => Message::Analyzed(
                        generation,
                        Err("Unexpected spectral worker response".into()),
                    ),
                })
            }
            Err(e) => {
                self.spectrum.error = Some(e);
                Task::none()
            }
        }
    }
    pub(super) fn fit_action(&mut self, action: FitAction) -> bool {
        let mut changed = false;
        let mut settings_changed = true;
        match action {
            FitAction::Select(name) => self.project.fit.function = name,
            FitAction::Method(method) => self.project.fit.method = method,
            FitAction::Text(field, value) => {
                let s = &mut self.project.fit;
                match field {
                    FitField::Start => s.interval[0] = value,
                    FitField::End => s.interval[1] = value,
                    FitField::Tolerance => s.tolerance = value,
                    FitField::MaxNodes => s.max_nodes = value,
                    FitField::Degree => s.degree = value,
                    FitField::Lawson => s.lawson = value,
                    FitField::LawsonDamping => s.lawson_damping = value,
                    FitField::SkSteps => s.sk_steps = value,
                    FitField::WfSteps => s.wf_steps = value,
                    FitField::RefinementTolerance => s.refinement_tolerance = value,
                };
            }
            FitAction::Fixed(v) => {
                self.project.fit.fixed_degree = v;
                if self.project.fit.method == aaa::Method::Classic {
                    self.project.fit.lawson = if v { "20" } else { "0" }.into();
                }
            }
            FitAction::Sign(v) => self.project.fit.sign = v,
            FitAction::Cleanup(v) => self.project.fit.cleanup = v,
            FitAction::Density(v) => self.project.fit.samples_per_gap = v,
            FitAction::Preview(v) => {
                self.fit.preview = v;
                changed = true;
                settings_changed = false;
            }
            FitAction::Refit => {
                self.fit.preview = true;
                self.queue_fit(true);
                settings_changed = false;
            }
            FitAction::Cancel => {
                self.invalidate_fit();
                self.fit.cancelled = true;
                settings_changed = false;
            }
            FitAction::Apply => {
                settings_changed = false;
                if let Some(fit) = &self.fit.result {
                    let mut candidate = self.project.clone();
                    candidate.entries = fit_entries(&fit.nodes);
                    candidate.entry_order.clear();
                    match candidate.compile() {
                        Ok(_) => {
                            self.project = candidate;
                            self.leave_fit_preview();
                            self.tab = Tab::Nodes;
                            changed = true;
                        }
                        Err(e) => self.fit.error = Some(format!("Cannot use fitted nodes: {e}")),
                    }
                }
            }
            FitAction::Frame => {
                self.frame_fit();
                changed = true;
                settings_changed = false;
            }
        }
        if settings_changed {
            self.invalidate_fit();
        }
        changed
    }
    pub(super) fn frame_fit(&mut self) {
        let Some(fit) = &self.fit.result else { return };
        let [a, b] = fit.interval;
        let model = Compiled::from_nodes(fit.nodes.clone(), crate::expression::Variables::new())
            .expect("validated fit");
        let (mut xmin, mut xmax, mut ymin, mut ymax) = (a, b, -0.25_f64, 0.25_f64);
        for i in 0..257 {
            let x = a + (b - a) * i as f64 / 256.;
            if let Some(y) = model.evaluate(C::new(x, 0.)) {
                if self.project.settings.mode == Mode::Real {
                    ymin = ymin.min(y.re);
                    ymax = ymax.max(y.re);
                } else {
                    xmin = xmin.min(y.re);
                    xmax = xmax.max(y.re);
                    ymin = ymin.min(y.im);
                    ymax = ymax.max(y.im);
                }
            }
        }
        if self.project.settings.mode == Mode::Complex {
            let span = (xmax - xmin).max(ymax - ymin);
            let cx = (xmin + xmax) * 0.5;
            let cy = (ymin + ymax) * 0.5;
            xmin = cx - span * 0.5;
            xmax = cx + span * 0.5;
            ymin = cy - span * 0.5;
            ymax = cy + span * 0.5;
        } else {
            ymin = ymin.min(-2.);
            ymax = ymax.max(2.);
        }
        let mx = (xmax - xmin).max(1e-6) * 0.12;
        let my = (ymax - ymin).max(1e-6) * 0.12;
        let bounds = Bounds {
            x_min: xmin - mx,
            x_max: xmax + mx,
            y_min: ymin - my,
            y_max: ymax + my,
        };
        let bounds = self.project.settings.guides.mapping.fit(bounds);
        if self.project.settings.guides.mapping.valid(bounds) {
            self.project.settings.bounds = bounds;
            self.axis = axis_strings(bounds);
        }
    }
}
pub fn fit_entries(nodes: &[model::Node]) -> Vec<model::Entry> {
    nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let mut entry = Entry::new(i);
            entry.name = format!("aaa{}", i + 1);
            entry.expressions = [n.z, n.value, n.weight].map(|z| model::number(C::new(z.re, 0.)));
            entry.imaginary =
                Some([n.z, n.value, n.weight].map(|z| model::number(C::new(z.im, 0.))));
            entry
        })
        .collect()
}

impl App {
    pub(super) fn receive_fit_progress(
        &mut self,
        generation: u64,
        step: aaa::Progress,
    ) -> Task<Message> {
        if generation == self.fit.generation {
            if let crate::numerics::aaa::Progress::Iteration(p) = &step {
                self.fit.history.push(p.clone());
            }
            self.fit.progress = Some(step);
        }
        Task::none()
    }
    pub(super) fn receive_fit(
        &mut self,
        generation: u64,
        result: Result<Option<aaa::Fit>, String>,
    ) -> Task<Message> {
        if self.fit.running_generation != Some(generation) {
            return Task::none();
        }
        self.fit.process = None;
        self.fit.busy = false;
        self.fit.running_generation = None;
        if generation == self.fit.generation {
            match result {
                Ok(Some(fit)) => {
                    self.fit.stale = false;
                    self.fit.result = Some(Arc::new(fit));
                    self.fit.error = None;
                    self.rebuild();
                }
                Ok(None) => {}
                Err(e) => self.fit.error = Some(e),
            }
        }
        self.launch_all()
    }
    pub(super) fn receive_spectrum(
        &mut self,
        generation: u64,
        result: Result<Option<Spectrum>, String>,
    ) -> Task<Message> {
        if self.spectrum.running_generation != Some(generation) {
            return Task::none();
        }
        self.spectrum.process = None;
        self.spectrum.running_generation = None;
        self.spectrum.busy = false;
        if generation == self.spectrum.generation {
            match result {
                Ok(Some(spectrum)) => {
                    self.spectrum.available = self.spectrum.running_options;
                    self.spectrum.result = Some(Arc::new(spectrum));
                    self.spectrum.error = None;
                    self.visual_revision += 1;
                }
                Ok(None) => {}
                Err(e) => self.spectrum.error = Some(e),
            }
        }
        self.launch_all()
    }
}

#[cfg(test)]
mod tests;
