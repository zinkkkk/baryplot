//! Compile the current model, schedule samples, and present completed colour images.
use super::*;

#[derive(Clone, PartialEq)]
pub(super) struct DomainKey {
    pub(super) terms: Arc<[[C; 3]]>,
    pub(super) bounds: Bounds,
    pub(super) resolution: u32,
    pub(super) style: crate::appearance::ColourStyle,
    pub(super) mapping: crate::axes::Mapping,
}
/// Identifies numerical line inputs independently of UI edits and viewport motion.
#[derive(PartialEq)]
pub(super) struct LineKey {
    pub(super) terms: Arc<[[C; 3]]>,
    pub(super) mode: Mode,
    pub(super) paths: Vec<sampling::PathPlan>,
}
#[derive(Debug, Clone)]
pub struct RenderResult {
    pub(super) epoch: u64,
    pub(super) revision: u64,
    pub(super) domain_revision: u64,
    pub(super) complete: bool,
    pub(super) data: Option<Arc<RenderData>>,
}
#[derive(Debug, Clone, Copy)]
pub struct TextureUpload {
    pub(super) epoch: u64,
    pub(super) domain_revision: u64,
    pub(super) domain: Bounds,
    pub(super) elapsed_ms: f64,
}

impl App {
    pub(super) fn rebuild(&mut self) {
        self.sync_numeric_editors();
        self.visual_revision += 1;
        self.dirty = true;
        let result = self.preview_model().and_then(|m| {
            let mut axes = [0.; 4];
            for (value, source) in axes.iter_mut().zip(&self.axis) {
                *value = sampling::real(source, &m).map_err(|e| format!("Axis: {e}"))?;
            }
            let bounds = Bounds {
                x_min: axes[0],
                x_max: axes[1],
                y_min: axes[2],
                y_max: axes[3],
            };
            if !self.project.settings.guides.mapping.valid(bounds) {
                return Err("Axis bounds must increase in the selected scale; log10 requires positive values".into());
            }
            self.project.settings.bounds=bounds;
            sampling::plan(&self.project.settings, &m).map(|p| (m, p))
        });
        match result {
            Ok((m, p)) => {
                let terms: Arc<[[C; 3]]> = m
                    .nodes
                    .iter()
                    .map(|n| [n.z, n.value, n.weight])
                    .collect::<Vec<_>>()
                    .into();
                let line_key = LineKey {
                    terms: Arc::clone(&terms),
                    mode: self.project.settings.mode,
                    paths: p.paths.clone(),
                };
                let line_changed = self.line_key.as_ref() != Some(&line_key) || self.data.is_none();
                if line_changed {
                    self.revision += 1;
                    self.line_pending = !sampling::fits_inline(&m, &self.project.settings, &p);
                    if !self.line_pending {
                        let mut data = sampling::sample_line(&m, &self.project.settings, &p);
                        if let Some(old) = &self.data {
                            data.image = old.image.clone();
                            data.domain = old.domain;
                        }
                        self.data = Some(Arc::new(data));
                    } else {
                        // Keep the last complete curve while a large replacement is sampled.
                        self.data.get_or_insert_with(|| {
                            Arc::new(RenderData {
                                samples: Arc::from([]),
                                image: None,
                                domain: p.domain,
                                elapsed_ms: 0.,
                                missing: 0,
                            })
                        });
                    }
                    self.line_key = Some(line_key);
                }
                let domain_key = self
                    .project
                    .settings
                    .coloring_enabled()
                    .then_some(DomainKey {
                        terms,
                        bounds: p.domain,
                        resolution: self.project.settings.resolution,
                        style: self.project.settings.colour_style,
                        mapping: self.project.settings.guides.mapping,
                    });
                let domain_changed = domain_key != self.domain_key;
                if domain_changed {
                    self.domain_revision += 1;
                    self.domain_pending = domain_key.is_some();
                    self.domain_key = domain_key;
                }
                self.dirty = self.domain_pending || self.line_pending;
                if let Some(display) = &mut self.fit.display_project {
                    display.settings = self.project.settings.clone();
                }
                self.points = m.nodes.clone();
                if self.fit.preview && self.fit.result.is_some() {
                    self.refresh_invalid_inputs();
                } else {
                    self.issues.clear();
                }
                let mode = self.project.settings.mode;
                for path in &p.paths {
                    if let Some(input) = self.project.settings.path_mut(path.path) {
                        match path.kind {
                            model::InputKind::Curve => {
                                for (old, new) in input.control_points.iter_mut().zip(&path.points)
                                {
                                    old[0] = new[0];
                                    if mode == Mode::Complex {
                                        old[1] = new[1];
                                    }
                                }
                            }
                            model::InputKind::Point => {
                                if let Some(point) = path.points.first() {
                                    if input.control_points.is_empty() {
                                        input.control_points.push(*point);
                                    } else {
                                        input.control_points[0][0] = point[0];
                                        if mode == Mode::Complex {
                                            input.control_points[0][1] = point[1];
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                if !p.path_errors.is_empty() {
                    self.refresh_invalid_inputs();
                }
                if self.fit.preview && self.fit.result.is_some() {
                    self.points = m.nodes.clone();
                }
                self.compiled = Some(m);
                self.plan = Some(p);
                self.error = None;
            }
            Err(e) => {
                self.revision += 1;
                self.line_key = None;
                self.refresh_invalid_inputs();
                self.error = Some(e);
                self.compiled = None;
                self.plan = None;
                self.dirty = false;
                self.domain_pending = false;
                self.line_pending = false;
                self.domain_key = None;
                self.latest.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    pub(super) fn launch(&mut self) -> Task<Message> {
        if self.busy || !(self.domain_pending || self.line_pending) {
            return Task::none();
        }
        let (Some(model), Some(plan)) = (self.compiled.clone(), self.plan.clone()) else {
            return Task::none();
        };
        self.busy = true;
        self.dirty = false;
        let revision = self.revision;
        let domain_revision = self.domain_revision;
        let epoch = self.latest.load(Ordering::Relaxed);
        let mut settings = self.project.settings.clone();
        if settings.mode == Mode::Real {
            settings.real_coloring = self.domain_pending && settings.real_coloring;
        } else {
            settings.coloring = self.domain_pending && settings.coloring;
        }
        let line_pending = self.line_pending;
        let latest = Arc::clone(&self.latest);
        let (sender, receiver) = smol::channel::unbounded();
        rayon::spawn(move || {
            let line_sender = sender.clone();
            let result =
                sampling::generate(&model, &settings, &plan, epoch, &latest, move |data| {
                    if !line_pending {
                        return;
                    }
                    let _ = line_sender.try_send(Message::Rendered(RenderResult {
                        epoch,
                        revision,
                        domain_revision,
                        complete: false,
                        data: Some(Arc::new(data)),
                    }));
                });
            let _ = sender.try_send(Message::Rendered(RenderResult {
                epoch,
                revision,
                domain_revision,
                complete: true,
                data: result.map(Arc::new),
            }));
        });
        Task::run(receiver, |message| message)
    }
}

impl App {
    pub(super) fn present_render(&mut self, result: RenderResult) -> Task<Message> {
        let mut upload = None;
        if result.complete {
            self.busy = false;
        }
        if result.epoch == self.latest.load(Ordering::Relaxed)
            && self.error.is_none()
            && let Some(rendered) = result.data
            && let Some(current) = &self.data
        {
            let mut data = (**current).clone();
            // Never replace a current curve with older samples. Domain images may
            // lag a frame, but completing them during motion prevents starvation.
            if result.revision == self.revision && self.line_pending {
                data.samples = Arc::clone(&rendered.samples);
                data.missing = rendered.missing;
                data.elapsed_ms = rendered.elapsed_ms;
                self.visual_revision += 1;
                self.line_pending = false;
            }
            if result.complete
                && result.domain_revision >= self.image_revision
                && self.domain_key.is_some()
                && rendered.image.is_some()
            {
                // Keep displaying the pinned front texture until the replacement
                // allocation reports that its GPU upload has completed.
                upload = rendered.image.clone().map(|handle| {
                    (
                        TextureUpload {
                            epoch: result.epoch,
                            domain_revision: result.domain_revision,
                            domain: rendered.domain,
                            elapsed_ms: rendered.elapsed_ms,
                        },
                        handle,
                    )
                });
            }
            self.data = Some(Arc::new(data));
        }
        self.dirty = self.domain_pending || self.line_pending;
        if let Some((upload, handle)) = upload {
            self.busy = true;
            return iced::widget::image::allocate(handle)
                .map(move |allocation| Message::ImageReady(upload, allocation));
        }
        if result.complete {
            self.launch_all()
        } else {
            Task::none()
        }
    }
    pub(super) fn present_image(
        &mut self,
        upload: TextureUpload,
        result: Result<iced::widget::image::Allocation, iced::widget::image::Error>,
    ) -> Task<Message> {
        self.busy = false;
        if upload.epoch == self.latest.load(Ordering::Relaxed)
            && self.error.is_none()
            && upload.domain_revision >= self.image_revision
            && self.domain_key.is_some()
        {
            match result {
                Ok(allocation) => {
                    if let Some(current) = &self.data {
                        let mut data = (**current).clone();
                        data.image = Some(allocation.handle().clone());
                        data.domain = upload.domain;
                        data.elapsed_ms = upload.elapsed_ms;
                        self.data = Some(Arc::new(data));
                        self.image_allocation = Some(allocation);
                        self.image_revision = upload.domain_revision;
                        if upload.domain_revision == self.domain_revision {
                            self.domain_pending = false;
                        }
                    }
                }
                Err(error) => {
                    if upload.domain_revision == self.domain_revision {
                        self.domain_pending = false;
                    }
                    self.notice = format!("Colour image could not be displayed: {error}");
                }
            }
        }
        self.dirty = self.domain_pending || self.line_pending;
        self.launch_all()
    }
}
