//! Route user actions while preserving one rebuild/invalidation boundary per edit.
use super::*;

impl App {
    pub(super) fn update_inner(&mut self, message: Message) -> Task<Message> {
        let numeric_edit = matches!(&message, Message::EditNumeric(_,action) if action.is_edit());
        let fit_inputs_changed = numeric_edit
            || matches!(
                &message,
                Message::GlobalKind(..)
                    | Message::GlobalEdit(..)
                    | Message::AddGlobal
                    | Message::RemoveGlobal(..)
                    | Message::Mode(..)
                    | Message::Load
                    | Message::Preset(..)
            );
        let manual_edit = matches!(
            &message,
            Message::Edit(..)
                | Message::EditImaginary(..)
                | Message::Name(..)
                | Message::Enabled(..)
                | Message::Lock(..)
                | Message::Add
                | Message::Remove(..)
                | Message::ClearEntries
                | Message::Plot(Interaction::Move(Target::Parameter(..), _))
        );
        if manual_edit && (self.fit.preview || self.fit.busy) {
            self.leave_fit_preview();
        }
        let mut changed = true;
        match message {
            Message::Appearance(action) => {
                let old_style = self.project.settings.colour_style;
                changed = self.appearance_action(action);
                if self.project.settings.colour_style != old_style {
                    self.latest.fetch_add(1, Ordering::Relaxed);
                }
            }
            Message::HoverEntry(i, inside) => {
                let previous = self.hovered_entry;
                if inside {
                    self.hovered_entry = Some(i);
                    self.last_hovered_entry = Some(i);
                } else {
                    if self.hovered_entry == Some(i) {
                        self.hovered_entry = None;
                    }
                }
                if previous != self.hovered_entry {
                    self.visual_revision += 1;
                }
                changed = false;
            }
            Message::FrontEntry(i) => {
                self.front_entry = if self.front_entry == Some(i) {
                    None
                } else {
                    Some(i)
                };
                self.visual_revision += 1;
                changed = false;
            }
            Message::BeginEntryDrag(i) => {
                if i < self.project.entries.len() {
                    self.entry_drag = Some(EntryDrag { from: i });
                    self.hovered_entry = Some(i);
                    self.last_hovered_entry = Some(i);
                    self.visual_revision += 1;
                }
                changed = false;
            }
            Message::EndEntryDrag => {
                if self.entry_drag.take().is_some() {
                    self.hovered_entry = None;
                    self.visual_revision += 1;
                }
                changed = false;
            }
            Message::ReorderEntry(row, slot) => {
                self.reorder_entry(row, slot);
                changed = false;
            }
            Message::ScrollEntries(amount) => {
                if self.entry_drag.is_none() {
                    return Task::none();
                }
                return iced::advanced::widget::operate(
                    iced::advanced::widget::operation::scrollable::scroll_by(
                        iced::advanced::widget::Id::new("editor-scroll"),
                        iced::advanced::widget::operation::scrollable::AbsoluteOffset {
                            x: 0.,
                            y: amount,
                        },
                        iced::advanced::widget::operation::Animation::Instant,
                    ),
                );
            }
            Message::Escape => {
                if self.appearance_ui.picker.is_some() {
                    self.appearance_action(appearance::Action::PickerCancel);
                } else {
                    self.appearance_ui.panel = None;
                    self.entry_drag = None;
                }
                changed = false;
            }
            Message::Edit(i, j, v) => {
                if let Some(e) = self.project.entries.get_mut(i) {
                    e.expressions[j] = v;
                }
            }
            Message::EditImaginary(i, j, v) => {
                if let Some(e) = self.project.entries.get_mut(i) {
                    e.split_components();
                    if let Some(imaginary) = &mut e.imaginary {
                        imaginary[j] = v;
                    }
                }
            }
            Message::Name(i, v) => {
                if let Some(e) = self.project.entries.get_mut(i) {
                    e.name = v;
                }
            }
            Message::Enabled(i, v) => {
                if let Some(e) = self.project.entries.get_mut(i) {
                    e.enabled = v;
                }
            }
            Message::Visible(i, v) => {
                if let Some(e) = self.project.entries.get_mut(i) {
                    e.visible = v;
                }
                changed = false;
                self.visual_revision += 1;
            }
            Message::Lock(i, j, k, v) => {
                if let Some(e) = self.project.entries.get_mut(i) {
                    e.locks[j][k] = v;
                }
            }
            Message::Add => self.add_entry(),
            Message::ClearEntries => self.clear_entries(),
            Message::Remove(i) => self.remove_entry(i),
            Message::GlobalKind(i, kind) => self.set_definition_kind(i, kind),
            Message::EditNumeric(i, action) => {
                if !self.edit_numeric_definition(i, action) {
                    return Task::none();
                }
            }
            Message::GlobalEdit(i, field, value) => self.edit_definition(i, field, value),
            Message::AddGlobal => self.add_definition(),
            Message::RemoveGlobal(i) => self.remove_definition(i),
            Message::Mode(mode) => {
                if mode == self.project.settings.mode {
                    return Task::none();
                }
                let colour_enabled = self.project.settings.coloring_enabled();
                self.latest.fetch_add(1, Ordering::Relaxed);
                self.project.split_components();
                self.reset_numerical();
                self.project.settings.mode = mode;
                if mode == Mode::Real {
                    self.project.settings.real_coloring = colour_enabled;
                } else {
                    self.project.settings.coloring = colour_enabled;
                }
                self.draw_path = false;
            }
            Message::Tab(tab) => {
                self.panes.restore();
                self.spectrum.details_requested = tab == Tab::Spectrum && !self.spectrum.paused;
                self.tab = tab;
                changed = false;
            }
            Message::NavigateFocus(back) => return self.navigate_focus(back),
            Message::Undo | Message::Redo => {
                changed = false;
            }
            Message::PlotSize(size) => {
                let next = crate::plot::area(size).size();
                let previous = self.plot_size.replace(next);
                if let Some(old) = previous {
                    let bounds = crate::axes::resize_view(
                        self.project.settings.bounds,
                        self.project.settings.guides.mapping,
                        old,
                        next,
                    );
                    if bounds != self.project.settings.bounds {
                        self.project.settings.bounds = bounds;
                        self.axis = axis_strings(bounds);
                    } else {
                        changed = false;
                    }
                } else {
                    changed = false;
                }
            }
            Message::ToggleEditor => {
                if self.panes.maximized().is_some() {
                    self.panes.restore();
                } else {
                    let plot = self
                        .panes
                        .iter()
                        .find_map(|(&id, pane)| matches!(pane, Pane::Plot).then_some(id));
                    if let Some(plot) = plot {
                        self.panes.maximize(plot);
                    }
                }
                self.entry_drag = None;
                self.hovered_entry = None;
                self.visual_revision += 1;
                changed = false;
            }
            Message::Field(field, v) => match field {
                Field::Samples => self.input_mut().samples = v,
                Field::Start => self.input_mut().interval[0] = v,
                Field::End => self.input_mut().interval[1] = v,
                Field::Extrapolation => self.input_mut().extrapolation = v,
                Field::GridStep => self.project.settings.grid_step = v,
                Field::MarginX => self.project.settings.domain_margin[0] = v,
                Field::MarginY => self.project.settings.domain_margin[1] = v,
            },
            Message::Axis(i, value) => {
                self.axis[i] = value;
            }
            Message::Flag(flag, v) => {
                if matches!(flag, Flag::Poles | Flag::Zeros | Flag::Residues) {
                    self.spectrum.paused = false;
                    self.spectrum.details_requested = false;
                }
                let s = &mut self.project.settings;
                match flag {
                    Flag::Output => s.output = v,
                    Flag::InputPath => s.input_path = v,
                    Flag::WeightedValues => s.weighted_values = v,
                    Flag::Grid => s.grid = v,
                    Flag::UnitCircle => s.unit_circle = v,
                    Flag::Nodes => s.nodes = v,
                    Flag::Values => s.values = v,
                    Flag::Weights => s.weights = v,
                    Flag::Coloring => {
                        if !v {
                            self.latest.fetch_add(1, Ordering::Relaxed);
                        }
                        if s.mode == Mode::Real {
                            s.real_coloring = v;
                        } else {
                            s.coloring = v;
                        }
                    }
                    Flag::Poles => s.analysis.poles = v,
                    Flag::Zeros => s.analysis.zeros = v,
                    Flag::Residues => s.analysis.residues = v,
                }
                if matches!(flag, Flag::Poles | Flag::Zeros | Flag::Residues)
                    && !s.analysis.enabled()
                {
                    self.spectrum.paused = true;
                }
                if !matches!(flag, Flag::Coloring) {
                    changed = false;
                    self.visual_revision += 1;
                }
            }
            Message::LineOpacity(v) => {
                self.project.settings.line_opacity = v;
                changed = false;
                self.visual_revision += 1;
            }
            Message::ColorOpacity(v) => {
                self.project.settings.color_opacity = v;
                changed = false;
            }
            Message::Extent(v) => self.project.settings.domain_extent = v,
            Message::EditPath(index, edit) => {
                if self.project.settings.path(index).is_none() {
                    return Task::none();
                }
                if self.active_path != index {
                    self.select_path(index);
                    self.visual_revision += 1;
                }
                return self.update_inner(*edit);
            }
            Message::TogglePath(i) => {
                if self.project.settings.path(i).is_some() {
                    if !self.collapsed_paths.remove(&i) {
                        self.collapsed_paths.insert(i);
                        if self.active_path == i {
                            self.draw_path = false;
                        }
                    } else {
                        self.select_path(i);
                    }
                    self.visual_revision += 1;
                }
                changed = false;
            }
            Message::SelectPath(i) => {
                self.collapsed_paths.remove(&i);
                self.select_path(i);
                self.visual_revision += 1;
                changed = false;
            }
            Message::AddPath(kind) => self.add_path(kind),
            Message::RemovePath => self.remove_path(),
            Message::PathName(name) => {
                self.input_mut().name = name;
                changed = false;
            }
            Message::PathEnabled(enabled) => self.input_mut().enabled = enabled,
            Message::PathKind(kind) => {
                self.input_mut().input_kind = Some(kind);
                self.draw_path = false;
            }
            Message::Curve(v) => self.input_mut().curve = v,
            Message::ControlWeight(i, v) => {
                if let Some(w) = self.input_mut().curve_weights.get_mut(i) {
                    *w = v;
                }
            }
            Message::ControlAxis(i, j, v) => {
                if let Some(d) = self.control_drafts.get_mut(i) {
                    d[j] = v;
                }
                self.input_mut().point_expressions = Some(self.control_drafts.clone());
            }
            Message::ClearPath => {
                let p = self.input_mut();
                p.control_points.clear();
                p.curve_weights.clear();
                p.point_expressions = None;
                self.sync_control_drafts();
            }
            Message::AddControl => self.append_control(None),
            Message::ResetPath => {
                let mode = self.project.settings.mode;
                let kind = self.input().kind(mode);
                let p = self.input_mut();
                let name = p.name.clone();
                *p = model::InputPath {
                    name,
                    input_kind: Some(kind),
                    ..Default::default()
                };
                if kind == model::InputKind::Point {
                    p.control_points = vec![[0., 0.]];
                    p.curve_weights = vec!["1".into()];
                }
                self.sync_control_drafts();
            }
            Message::DrawPath(v) => {
                self.draw_path = v;
                changed = false;
            }
            Message::ResetView => {
                self.project.settings.bounds =
                    self.project.settings.guides.mapping.fit(Bounds::default());
                self.axis = axis_strings(self.project.settings.bounds);
            }
            Message::Plot(interaction) => match interaction {
                Interaction::BeginGesture | Interaction::Finish(_) => {
                    changed = false;
                }
                Interaction::Bounds(b) => {
                    if !self.project.settings.guides.mapping.valid(b) {
                        return Task::none();
                    }
                    self.project.settings.bounds = b;
                    self.axis = axis_strings(b);
                }
                Interaction::Move(Target::Parameter(i, j), z) => {
                    if !z.re.is_finite() || !z.im.is_finite() {
                        return Task::none();
                    }
                    let s = &self.project.settings;
                    if let Some(e) = self.project.entries.get_mut(i) {
                        if s.mode == Mode::Real {
                            let x = if e.locks[j][0] { z.re.round() } else { z.re };
                            e.expressions[j] = model::number(C::new(x, 0.));
                            e.real_y[j] = z.im;
                        } else {
                            let z = C::new(
                                if e.locks[j][0] { z.re.round() } else { z.re },
                                if e.locks[j][1] { z.im.round() } else { z.im },
                            );
                            e.split_components();
                            e.expressions[j] = model::number(C::new(z.re, 0.));
                            if let Some(imaginary) = &mut e.imaginary {
                                imaginary[j] = model::number(C::new(z.im, 0.));
                            }
                        }
                    }
                }
                Interaction::Move(Target::Interval(i), z) => self.move_interval(0, i, z),
                Interaction::Move(Target::PathInterval(path, i), z) => {
                    self.move_interval(path, i, z)
                }
                Interaction::Move(Target::Derived(_), _) => {}
                Interaction::Move(Target::Control(i), z) => self.move_control(0, i, z),
                Interaction::Move(Target::PathControl(path, i), z) => self.move_control(path, i, z),
                Interaction::AddControl(z) => self.append_control(Some(z)),
                Interaction::RemoveControl(i) => self.remove_control(self.active_path, i),
                Interaction::RemovePathControl(path, i) => self.remove_control(path, i),
            },
            Message::Resize(event) => {
                self.panes.resize(event.split, event.ratio);
                changed = false;
            }
            Message::Rendered(result) => return self.present_render(result),
            Message::ImageReady(upload, result) => return self.present_image(upload, result),
            Message::Spectrum(action) => {
                self.spectrum_action(action);
                changed = false;
            }
            Message::CancelSpectrum => {
                self.cancel_spectrum();
                changed = false;
            }
            Message::CalculateSpectrum => {
                self.spectrum.paused = false;
                self.spectrum.details_requested = true;
                self.spectrum.error = None;
                changed = false;
            }
            Message::Fit(action) => {
                changed = self.fit_action(action);
            }
            Message::FitProgress(generation, step) => {
                return self.receive_fit_progress(generation, step);
            }
            Message::Fitted(generation, result) => return self.receive_fit(generation, result),
            Message::Analyzed(generation, result) => {
                return self.receive_spectrum(generation, result);
            }
            Message::ProjectPath(v) => {
                self.path = v;
                changed = false;
            }
            Message::Save => {
                self.save_project();
                changed = false;
            }
            Message::Load => changed = self.load_project(),
            Message::Preset(mode) => self.use_preset(mode),
        }
        if changed {
            self.rebuild();
        }
        if fit_inputs_changed {
            self.invalidate_fit();
        }
        self.launch_all()
    }
}
