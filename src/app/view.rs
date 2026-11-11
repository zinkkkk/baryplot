use super::*;
mod nodes;
mod settings;
use crate::plot;
use iced::{
    Color, Element, Fill, Theme, Widget,
    widget::{
        self, button, checkbox, column, container, pick_list, row, scrollable, slider, space,
        stack, text, text_input,
    },
};
pub(super) const ACCENT: Color = Color::from_rgb(0.38, 0.84, 0.76);
pub(super) const WARNING: Color = Color::from_rgb(1.0, 0.52, 0.45);
pub(super) const MUTED: Color = Color::from_rgb(0.52, 0.59, 0.70);
impl App {
    pub fn view(&self) -> Element<'_, Message> {
        let s = &self.project.settings;
        let collapsed = self.panes.maximized().is_some();
        let guides = container(
            row![
                self.toggle(Flag::Grid, "Grid", s.grid),
                self.toggle(Flag::UnitCircle, "Unit circle", s.unit_circle),
                button(text("⚙").size(14))
                    .padding([2, 4])
                    .style(button::text)
                    .on_press(Message::Appearance(appearance::Action::Open(
                        appearance::Panel::Guides
                    )))
            ]
            .spacing(7)
            .align_y(iced::Center),
        )
        .padding([3, 7])
        .style(display_group);
        let points = container(
            row![
                self.toggle(Flag::Nodes, "Nodes", s.nodes),
                self.toggle(Flag::Values, "Values", s.values),
                self.toggle(Flag::Weights, "Weights", s.weights),
                self.toggle(Flag::WeightedValues, "w×v", s.weighted_values),
            ]
            .spacing(7)
            .align_y(iced::Center),
        )
        .padding([3, 7])
        .style(display_group);
        let mut curves = row![
            self.toggle(Flag::InputPath, "Input", s.input_path),
            self.toggle(Flag::Output, "Output", s.output)
        ]
        .spacing(7)
        .align_y(iced::Center);
        curves = curves.push(
            button(text("⚙").size(13))
                .padding([2, 4])
                .on_press(Message::Appearance(appearance::Action::Open(
                    appearance::Panel::Lines,
                )))
                .style(button::text)
                .boxed(),
        );
        curves = curves.push(self.toggle(Flag::Coloring, "Colour", s.coloring_enabled()));
        curves = curves.push(
            button(text("⚙").size(13))
                .padding([2, 4])
                .on_press(Message::Appearance(appearance::Action::Open(
                    appearance::Panel::Colour,
                )))
                .style(button::text)
                .boxed(),
        );
        let curves = container(curves).padding([3, 7]).style(display_group);
        let mut spectrum_controls = row![
            self.toggle(Flag::Poles, "Poles", s.analysis.poles),
            self.toggle(Flag::Zeros, "Zeros", s.analysis.zeros),
            self.toggle(Flag::Residues, "Residues", s.analysis.residues),
            widget::tooltip(
                button(text("⚙").size(13))
                    .padding([2, 4])
                    .on_press(Message::Tab(Tab::Spectrum))
                    .style(if self.tab == Tab::Spectrum {
                        button::primary
                    } else {
                        button::text
                    }),
                text("Poles, zeros and residues settings").size(12),
                widget::tooltip::Position::Bottom
            ),
        ]
        .spacing(7)
        .align_y(iced::Center);
        if self.spectrum.busy || self.spectrum.pending.is_some() {
            spectrum_controls = spectrum_controls.push(
                widget::tooltip(
                    button(text("×").size(15))
                        .padding([1, 4])
                        .on_press(Message::CancelSpectrum)
                        .style(button::danger),
                    text("Cancel spectrum calculation").size(12),
                    widget::tooltip::Position::Bottom,
                )
                .boxed(),
            );
        }
        let spectrum = container(spectrum_controls)
            .padding([3, 7])
            .style(display_group);
        let tools = row![
            widget::tooltip(
                button(text(if collapsed { "›" } else { "‹" }).size(18))
                    .width(24)
                    .padding([1, 3])
                    .style(button::secondary)
                    .on_press(Message::ToggleEditor),
                text(if collapsed {
                    "Show inputs"
                } else {
                    "Hide inputs"
                })
                .size(12),
                widget::tooltip::Position::Bottom
            ),
            pick_list(
                Some(s.mode),
                [Mode::Real, Mode::Complex],
                ToString::to_string
            )
            .on_select(Message::Mode)
            .width(104)
            .text_size(12),
            guides,
            points,
            curves,
            spectrum,
        ]
        .spacing(7)
        .align_y(iced::Center);
        let header = container(
            row![
                tools,
                space().width(Fill),
                button(text("Reset view").size(11))
                    .style(button::secondary)
                    .padding([6, 10])
                    .on_press(Message::ResetView)
            ]
            .spacing(6)
            .align_y(iced::Center),
        )
        .padding([9, 12]);
        let panes = pane_grid::PaneGrid::new(&self.panes, |_, pane, _| {
            pane_grid::Content::new(match pane {
                Pane::Editor => self.editor(),
                Pane::Plot => self.plot_view(),
            })
        })
        .spacing(7)
        .min_size(400)
        .on_resize(8, Message::Resize);
        let status = if let Some(error) = &self.error {
            format!(
                "●  {error}  ·  {}",
                if !self.points.is_empty() {
                    "Output held at last valid inputs; points remain editable"
                } else {
                    "Output awaits valid inputs"
                }
            )
        } else if self.busy || self.dirty {
            "●  LIVE   Updating…".into()
        } else if let Some(data) = &self.data {
            format!(
                "●  LIVE   {} active nodes   ·   {} samples   ·   {:.1} ms   ·   {} undefined samples",
                self.compiled.as_ref().map_or(0, |m| m.nodes.len()),
                data.samples.len(),
                data.elapsed_ms,
                data.missing
            )
        } else {
            "●  LIVE".into()
        };
        let footer = container(
            row![
                text(status).size(11).color(if self.error.is_some() {
                    Color::from_rgb(1., 0.57, 0.53)
                } else {
                    ACCENT
                }),
                space().width(Fill),
                widget::tooltip(
                    button(text("Undo").size(11))
                        .padding([3, 6])
                        .style(button::secondary)
                        .on_press_maybe(self.history.can_undo().then_some(Message::Undo)),
                    text("Ctrl+Z").size(11),
                    widget::tooltip::Position::Top
                ),
                widget::tooltip(
                    button(text("Redo").size(11))
                        .padding([3, 6])
                        .style(button::secondary)
                        .on_press_maybe(self.history.can_redo().then_some(Message::Redo)),
                    text("Ctrl+Shift+Z / Ctrl+Y").size(11),
                    widget::tooltip::Position::Top
                ),
                text("Wheel: zoom  ·  Shift: faster  ·  Drag: pan  ·  Axis labels: scale")
                    .size(11)
                    .color(MUTED)
            ]
            .spacing(8),
        )
        .padding([9, 18]);
        let main = column![
            header,
            container(panes).padding([0, 10]).height(Fill),
            footer
        ];
        let settings: Element<'_, Message> = self
            .appearance_ui
            .panel
            .map_or_else(|| space().boxed(), |panel| self.appearance_dialog(panel));
        let picker: Element<'_, Message> = self
            .appearance_ui
            .picker
            .as_ref()
            .map_or_else(|| space().boxed(), |picker| picker.view());
        let backdrop = |visible: bool, opacity: f32, message: Message| -> Element<'_, Message> {
            if !visible {
                return space().boxed();
            }
            widget::opaque(
                widget::mouse_area(container(space().width(Fill).height(Fill)).style(move |_| {
                    container::Style {
                        background: Some(Color::from_rgba(0., 0., 0., opacity).into()),
                        ..Default::default()
                    }
                }))
                .on_press(message),
            )
            .boxed()
        };
        // Stable sibling layers retain each dialog's position and the underlying UI state.
        stack![
            main,
            backdrop(
                self.appearance_ui.panel.is_some(),
                0.5,
                Message::Appearance(appearance::Action::Close)
            ),
            crate::dialog::Dialog::new(
                widget::opaque(settings),
                self.appearance_ui.panel.is_some()
            ),
            backdrop(
                self.appearance_ui.picker.is_some(),
                0.55,
                Message::Appearance(appearance::Action::PickerCancel)
            ),
            crate::dialog::Dialog::new(widget::opaque(picker), self.appearance_ui.picker.is_some()),
        ]
        .boxed()
    }

    fn editor(&self) -> Element<'_, Message> {
        let mut tabs = row![].spacing(3);
        for (tab, label) in [
            (Tab::Nodes, "Nodes"),
            (Tab::Definitions, "Definitions"),
            (Tab::Fit, "AAA fit"),
            (Tab::Plot, "Plot"),
            (Tab::Path, "Input paths"),
            (Tab::Help, "Guide"),
        ] {
            tabs = tabs.push(
                button(text(label).size(11))
                    .padding([7, 6])
                    .style(if self.tab == tab {
                        button::primary
                    } else {
                        button::text
                    })
                    .on_press(Message::Tab(tab))
                    .boxed(),
            );
        }
        let content = match self.tab {
            Tab::Nodes => self.nodes_view(),
            Tab::Definitions => self.definitions_view(),
            Tab::Fit => self.fitting_view(),
            Tab::Spectrum => self.spectrum_view(),
            Tab::Plot => self.settings_view(),
            Tab::Path => self.path_view(),
            Tab::Help => self.help_view(),
        };
        let mut editor = column![container(tabs).padding([10, 12])];
        if self.tab == Tab::Nodes && !(self.fit.preview && self.fit.result.is_some()) {
            editor = editor.push(container(self.nodes_header()).padding([6, 16]).boxed());
        }
        if self.tab == Tab::Path {
            editor = editor.push(container(self.paths_header()).padding([6, 16]).boxed());
        }
        editor = editor.push(
            scrollable(container(content).padding([6, 16]))
                .id("editor-scroll")
                .height(Fill)
                .boxed(),
        );
        container(editor)
            .id("input-editor")
            .style(panel)
            .height(Fill)
            .boxed()
    }

    fn axis_field(&self, i: usize, label: &'static str) -> Element<'_, Message> {
        column![
            text(label).size(11).color(MUTED),
            self.input_field(InputKey::Axis(i), label, &self.axis[i], move |v| {
                Message::Axis(i, v)
            })
        ]
        .spacing(5)
        .width(Fill)
        .boxed()
    }
    pub(super) fn setting<'a>(
        &self,
        field: Field,
        label: &'static str,
        value: &'a str,
    ) -> Element<'a, Message> {
        column![
            text(label).size(11).color(MUTED),
            self.input_field(InputKey::Setting(field), label, value, move |v| {
                Message::Field(field, v)
            })
        ]
        .spacing(5)
        .width(Fill)
        .boxed()
    }
    fn warning(&self, key: InputKey) -> Element<'static, Message> {
        if let Some(error) = self.issues.get(&key).cloned() {
            widget::tooltip(
                text("!").size(13).color(WARNING).width(9),
                container(text(error).size(12)).padding(8).style(card),
                widget::tooltip::Position::Bottom,
            )
            .boxed()
        } else {
            space().width(9).boxed()
        }
    }
    pub(super) fn input_field<'a>(
        &self,
        key: InputKey,
        placeholder: &'a str,
        value: impl iced::advanced::text::IntoFragment<'a>,
        on_input: impl Fn(String) -> Message + 'a,
    ) -> Element<'a, Message> {
        let invalid = self.issues.contains_key(&key);
        let compact = matches!(key, InputKey::Parameter(..) | InputKey::Name(..));
        let input = text_input(placeholder, value)
            .on_input(on_input)
            .size(if compact { 12 } else { 13 })
            .padding(if compact { 4 } else { 8 })
            .style(move |theme, status| {
                let mut style = text_input::default(theme, status);
                if invalid {
                    style.border.color = WARNING;
                }
                style
            });
        row![input, self.warning(key)]
            .align_y(iced::Center)
            .spacing(4)
            .boxed()
    }
    fn plot_view(&self) -> Element<'_, Message> {
        let background: Element<'_, Message> = widget::canvas(plot::Backdrop {
            project: self.displayed_project(),
            data: self.data.as_deref(),
        })
        .width(Fill)
        .height(Fill)
        .boxed();
        let foreground = widget::canvas(plot::Plot {
            project: self.displayed_project(),
            model: self.compiled.as_ref(),
            points: &self.points,
            plan: self.plan.as_ref(),
            data: self.data.as_deref().filter(|_| self.error.is_none()),
            revision: self.visual_revision,
            draw_path: self.draw_path,
            spectrum: self.spectrum.result.as_deref(),
            spectrum_filter: self.spectrum.filter,
            editable_points: !(self.fit.preview && self.fit.result.is_some()),
            input_priority: self.tab == Tab::Path || self.draw_path,
            active_path: self.active_path,
            emphasized_entry: self.foreground_entry(),
            highlighted_entry: self.hovered_entry,
        })
        .width(Fill)
        .height(Fill)
        .map(Message::Plot);
        container(widget::sensor(stack![background, foreground]).on_resize(Message::PlotSize))
            .padding(1)
            .style(plot_border)
            .height(Fill)
            .boxed()
    }
    pub(super) fn toggle(
        &self,
        flag: Flag,
        label: &'static str,
        value: bool,
    ) -> Element<'_, Message> {
        checkbox(value)
            .label(label)
            .size(12)
            .text_size(11)
            .spacing(5)
            .on_toggle(move |v| Message::Flag(flag, v))
            .boxed()
    }
}
fn panel(_: &Theme) -> container::Style {
    container::Style::default()
        .background(Color::from_rgb(0.065, 0.082, 0.116))
        .border(iced::Border {
            radius: 10.into(),
            ..Default::default()
        })
}
pub(super) fn card(_: &Theme) -> container::Style {
    container::Style::default()
        .background(Color::from_rgb(0.086, 0.106, 0.148))
        .border(iced::Border {
            color: Color::from_rgb(0.15, 0.18, 0.24),
            width: 1.,
            radius: 4.into(),
        })
}
fn plot_border(_: &Theme) -> container::Style {
    container::Style::default()
        .background(Color::from_rgb(0.039, 0.055, 0.083))
        .border(iced::Border {
            color: Color::from_rgb(0.24, 0.31, 0.41),
            width: 1.,
            radius: 8.into(),
        })
}

fn display_group(_: &Theme) -> container::Style {
    container::Style::default().border(iced::Border {
        color: Color::from_rgb(0.23, 0.29, 0.38),
        width: 1.,
        radius: 5.into(),
    })
}
