use super::view::{ACCENT, MUTED, WARNING};
use super::*;
use iced::{
    Element, Fill, Widget,
    widget::{button, checkbox, column, container, pick_list, row, space, text, text_input},
};
impl App {
    pub(super) fn fitting_view(&self) -> Element<'_, Message> {
        let s = &self.project.fit;
        let nonlinear = s.method == crate::numerics::aaa::Method::Nonlinear;
        let definitions = self.project.resolve_definitions();
        let lawson = crate::expression::evaluate(&s.lawson, &definitions.variables);
        let lawson_enabled = lawson.as_ref().is_ok_and(|n| n.im == 0. && n.re > 0.);
        let cleanup_available = !s.fixed_degree && lawson.is_ok_and(|n| n == C::new(0., 0.));
        let functions: Vec<_> = self
            .project
            .globals
            .iter()
            .filter(|g| {
                g.kind.is_function()
                    && crate::expression::parameters(&g.parameters).is_ok_and(|a| a.len() == 1)
            })
            .map(|g| g.name.clone())
            .collect();
        let selected = functions.iter().find(|name| **name == s.function).cloned();
        let mut view=column![
            text("AAA function fitting").size(18),
            text("Choose a function and interval, then run once. Samples refine locally between support points; edits wait for the next run.").size(12).color(MUTED),
            column![
                text("Method").size(11).color(MUTED),
                pick_list(Some(s.method),[crate::numerics::aaa::Method::Classic,crate::numerics::aaa::Method::Nonlinear],ToString::to_string)
                    .on_select(|method|Message::Fit(FitAction::Method(method))).width(Fill).text_size(13)
            ].spacing(5),
            text("Function · one argument").size(11).color(MUTED),
            pick_list(selected,functions,ToString::to_string).on_select(|name|Message::Fit(FitAction::Select(name))).placeholder("Choose a defined function").width(Fill).text_size(13),
            row![self.fit_setting(FitField::Start,"Interval start",&s.interval[0]),self.fit_setting(FitField::End,"Interval end",&s.interval[1])].spacing(10),
            self.fit_setting(FitField::Tolerance,"Relative tolerance · f64",&s.tolerance),
            checkbox(s.fixed_degree).label("Fixed degree (N,N) · run to N + 1 support points").size(14).text_size(12).on_toggle(|v|Message::Fit(FitAction::Fixed(v))),
        ].spacing(11);
        if s.fixed_degree {
            view = view.push(self.fit_setting(FitField::Degree, "Degree N · 0–255", &s.degree));
        } else {
            view = view.push(self.fit_setting(
                FitField::MaxNodes,
                "Max support nodes · 1–256",
                &s.max_nodes,
            ));
        }
        if nonlinear {
            view = view.push(container(column![
                text("Nonlinear weight refinement").size(13),
                row![self.fit_setting(FitField::SkSteps,"SK iterations · 1–100",&s.sk_steps),self.fit_setting(FitField::WfSteps,"Whitfield iterations · 1–100",&s.wf_steps)].spacing(8),
                self.fit_setting(FitField::RefinementTolerance,"Refinement convergence tolerance",&s.refinement_tolerance),
                text("Refines rational L2 error using the paper's adaptive SK/WF initialization. Failed refinements keep the previous fit and use relative-error support selection. Keeps earlier local probes as constraints; max-error tolerance is verified independently.").size(11).color(MUTED),
            ].spacing(8)).padding(9).style(super::view::card).boxed());
        }
        view = view
            .push(
                row![
                    self.fit_setting(FitField::Lawson, "Lawson steps · 0–100", &s.lawson),
                    column![
                        text("Minimum samples per gap").size(11).color(MUTED),
                        pick_list(
                            Some(s.samples_per_gap),
                            [3, 5, 9, 15, 30],
                            ToString::to_string
                        )
                        .on_select(|n| Message::Fit(FitAction::Density(n)))
                        .text_size(13)
                        .width(Fill)
                    ]
                    .spacing(5)
                    .width(Fill)
                ]
                .spacing(10)
                .boxed(),
            )
            .push(
                checkbox(s.cleanup && cleanup_available)
                    .label(if s.fixed_degree {
                        "Cleanup unavailable · fixed degree preserves support count"
                    } else if !cleanup_available {
                        "Cleanup unavailable · set Lawson steps to 0"
                    } else {
                        "Cleanup · remove Froissart doublets"
                    })
                    .size(14)
                    .text_size(11)
                    .on_toggle_maybe(
                        cleanup_available.then_some(|v| Message::Fit(FitAction::Cleanup(v))),
                    )
                    .boxed(),
            );
        if !nonlinear {
            view = view.push(
                checkbox(s.sign)
                    .label("Sign-function modification")
                    .size(14)
                    .text_size(12)
                    .on_toggle(|v| Message::Fit(FitAction::Sign(v)))
                    .boxed(),
            );
        }
        if lawson_enabled {
            view = view.push(self.fit_setting(FitField::LawsonDamping,"Lawson damping · 0–1 (1 = standard)",&s.lawson_damping))
                .push(text("Smaller damping blends each Lawson reweighting with the previous row weights. 0 keeps them uniform. Lawson is optional post-processing and may change support values.").size(11).color(MUTED).boxed());
        }
        let mut actions = row![
            button(
                text(if self.fit.busy {
                    "Restart fit"
                } else {
                    "Run fit"
                })
                .size(12)
            )
            .on_press_maybe((!s.function.is_empty()).then_some(Message::Fit(FitAction::Refit)))
            .style(button::secondary)
        ]
        .spacing(8);
        if self.fit.busy || self.fit.pending.is_some() {
            actions = actions.push(
                button(text("Cancel").size(12))
                    .on_press(Message::Fit(FitAction::Cancel))
                    .style(button::danger)
                    .boxed(),
            );
        }
        view = view.push(actions.boxed());
        if self.fit.cancelled {
            view = view.push(
                text("Fit cancelled. Completed results remain available.")
                    .size(11)
                    .color(MUTED)
                    .boxed(),
            );
        }
        if s.function.is_empty() {
            view=view.push(text("Add a one-argument function in Definitions, e.g. target(x) = exp(x). Functions can return real or complex values; select the corresponding toolbar mode.").size(12).color(MUTED).boxed());
        }
        if self.fit.stale && !self.fit.cancelled && !self.fit.busy && self.fit.pending.is_none() {
            view = view.push(
                text("Settings changed · press Run fit. The last completed result is retained.")
                    .size(11)
                    .color(MUTED)
                    .boxed(),
            );
        }
        if self.fit.busy || self.fit.pending.is_some() {
            let status = if self.fit.pending.is_some() && self.fit.busy {
                "Starting replacement fit…".into()
            } else if self.fit.running_generation != Some(self.fit.generation) {
                "Starting fit…".into()
            } else {
                self.fit
                    .progress
                    .as_ref()
                    .map_or("Starting fit…".into(), |p| p.description())
            };
            view = view.push(text(status).size(12).color(ACCENT).boxed());
        }
        if let Some(error) = &self.fit.error {
            view = view.push(text(format!("! {error}")).size(12).color(WARNING).boxed());
        }
        if let Some(fit) = &self.fit.result {
            view=view.push(text(format!("{} · {} · {} nodes · type ≤ ({},{})",fit.method,fit.function,fit.nodes.len(),fit.nodes.len().saturating_sub(1),fit.nodes.len().saturating_sub(1))).size(14).boxed())
                .push(text(format!("Interval [{}, {}] · max error {:.3e} · relative {:.3e}\nVerification relative L2 {:.3e}\n{} adaptive samples · {} verification samples · {} distinct evaluations · {:.1} ms\n{}",fit.interval[0],fit.interval[1],fit.error,fit.relative_error,fit.relative_l2,fit.grid,fit.validation_samples,fit.evaluations,fit.elapsed_ms,fit.note)).size(11).color(if fit.converged {ACCENT}else{MUTED}).boxed())
                .push(checkbox(self.fit.preview).label("Show fit preview in the plot").size(14).text_size(12).on_toggle(|v|Message::Fit(FitAction::Preview(v))).boxed())
                .push(row![button(text("Use as editable nodes").size(12)).on_press(Message::Fit(FitAction::Apply)),button(text("Frame fit interval").size(12)).on_press(Message::Fit(FitAction::Frame)).style(button::secondary)].spacing(8).boxed());
        }
        let convergence = if self.fit.busy && !self.fit.history.is_empty() {
            Some(self.fit.history.as_slice())
        } else {
            self.fit.result.as_ref().map(|fit| fit.history.as_slice())
        };
        if let Some(convergence) = convergence {
            let recent: Vec<_> = convergence.iter().rev().take(8).collect();
            let mut history = column![
                text("Recent convergence · nodes / samples / max error / rel. L2")
                    .size(11)
                    .color(MUTED)
            ]
            .spacing(4);
            for step in recent.into_iter().rev() {
                history = history.push(
                    text(format!(
                        "{}   /   {}   /   {:.3e}   /   {:.3e}",
                        step.nodes, step.grid, step.error, step.relative_l2
                    ))
                    .size(11)
                    .color(MUTED)
                    .boxed(),
                );
            }
            view = view.push(history.boxed());
        }
        view.push(text("NL-AAA minimizes sampled L2; adaptive grids and optional Lawson/cleanup mean the fixed-grid monotonicity claim is not a global guarantee. Fixed degree disables early tolerance stopping and cleanup; exact algebraic degree may be lower after cancellations. Lawson changes support values to improve the fit. Tolerance is checked on denser and off-grid samples, not guaranteed mathematically over every point.").size(11).color(MUTED).boxed())
.push(space().height(10).boxed()).boxed()
    }
    fn fit_setting<'a>(
        &self,
        field: FitField,
        label: &'static str,
        value: &'a str,
    ) -> Element<'a, Message> {
        column![
            text(label).size(11).color(MUTED),
            text_input("expression", value)
                .on_input(move |v| Message::Fit(FitAction::Text(field, v)))
                .size(13)
                .padding(8)
        ]
        .spacing(5)
        .width(Fill)
        .boxed()
    }
    pub(super) fn fitted_nodes_view(&self) -> Element<'_, Message> {
        let mut view=column![text(format!("Fitted nodes · {} entries", self.points.len())).size(16),text("These are the fitted barycentric parameters. Use them as editable nodes to drag or edit them; your manual setup is retained until then.").size(12).color(MUTED),row![button(text("Use as editable nodes").size(12)).on_press(Message::Fit(FitAction::Apply)),button(text("Show manual setup").size(12)).on_press(Message::Fit(FitAction::Preview(false))).style(button::secondary)].spacing(8)].spacing(12);
        if let Some(error) = &self.fit.error {
            view = view.push(text(error).size(12).color(WARNING).boxed());
        }
        for (i, n) in self.points.iter().enumerate() {
            let focused = self.hovered_entry == Some(n.row);
            view = view.push(
                iced::widget::mouse_area(
                    container(
                        column![
                            row![
                                text(format!("aaa{}", i + 1))
                                    .size(12)
                                    .color(ACCENT)
                                    .width(Fill),
                                button(text("↑").size(13))
                                    .padding([1, 5])
                                    .on_press(Message::FrontEntry(n.row))
                                    .style(button::text)
                            ],
                            text(format!(
                                "n: {}\nv: {}\nw: {}",
                                model::number(n.z),
                                model::number(n.value),
                                model::number(n.weight)
                            ))
                            .size(11)
                        ]
                        .spacing(3),
                    )
                    .padding(8)
                    .style(move |theme| {
                        let mut s = super::view::card(theme);
                        if focused {
                            s.border.color = ACCENT;
                        }
                        s
                    }),
                )
                .on_enter(Message::HoverEntry(n.row, true))
                .on_exit(Message::HoverEntry(n.row, false))
                .boxed(),
            );
        }
        view.boxed()
    }
}
