//! Independently collapsible input-path cards with edits routed to their own path.
use super::view::{ACCENT, MUTED, WARNING, card};
use super::*;
use crate::model::{InputKind, InputPath};
use iced::{
    Element, Fill, Widget,
    widget::{
        button, checkbox, column, container, pick_list, row, space, text, text_input, tooltip,
    },
};

fn edit(path: usize, message: Message) -> Message {
    Message::EditPath(path, Box::new(message))
}
impl App {
    pub(super) fn paths_header(&self) -> Element<'_, Message> {
        let count = self.project.settings.additional_paths.len() + 1;
        row![
            text(format!("Input paths · {count}")).size(17).width(Fill),
            button(text("+ Add path").size(12))
                .style(button::primary)
                .on_press_maybe((count < 32).then_some(Message::AddPath(
                    self.input().kind(self.project.settings.mode)
                )))
        ]
        .align_y(iced::Center)
        .spacing(8)
        .boxed()
    }
    pub(super) fn path_view(&self) -> Element<'_, Message> {
        let mut paths = column![].spacing(10);
        let mode = self.project.settings.mode;
        for (index, path) in self.project.settings.paths().enumerate() {
            let collapsed = self.collapsed_paths.contains(&index);
            let selected = self.active_path == index;
            let error = self.plan.as_ref().and_then(|p| p.path_errors.get(&index));
            let header = row![
                tooltip(
                    button(text(if collapsed { "▸" } else { "▾" }).size(16))
                        .padding([2, 5])
                        .style(button::text)
                        .on_press(Message::TogglePath(index)),
                    text(if collapsed {
                        "Expand path"
                    } else {
                        "Collapse path"
                    })
                    .size(11),
                    tooltip::Position::Bottom,
                ),
                text_input("Path name", &path.name)
                    .size(13)
                    .padding(5)
                    .on_input(move |v| edit(index, Message::PathName(v))),
                checkbox(path.enabled)
                    .label("On")
                    .size(13)
                    .text_size(11)
                    .on_toggle(move |v| edit(index, Message::PathEnabled(v))),
                tooltip(
                    button(text("×").size(15))
                        .padding([2, 5])
                        .style(button::danger)
                        .on_press_maybe(
                            (!self.project.settings.additional_paths.is_empty())
                                .then_some(edit(index, Message::RemovePath))
                        ),
                    text("Remove path").size(11),
                    tooltip::Position::Bottom,
                ),
            ]
            .align_y(iced::Center)
            .spacing(6);
            let summary = match path.kind(mode) {
                InputKind::Interval => format!(
                    "Real interval · {} → {}",
                    path.interval[0], path.interval[1]
                ),
                InputKind::Curve => {
                    format!("{} · {} points", path.curve, path.control_points.len())
                }
                InputKind::Point => "Single point".into(),
            };
            let mut contents = column![
                header,
                button(text(summary).size(11).color(if error.is_some() {
                    WARNING
                } else if selected {
                    ACCENT
                } else {
                    MUTED
                }))
                .style(button::text)
                .padding([1, 4])
                .width(Fill)
                .on_press(Message::SelectPath(index)),
            ]
            .spacing(6);
            if let Some(error) = error {
                contents =
                    contents.push(text(format!("! {error}")).size(11).color(WARNING).boxed());
            }
            if !collapsed {
                contents = contents.push(
                    self.path_controls(index, path)
                        .map(move |message| edit(index, message))
                        .boxed(),
                );
            }
            paths = paths.push(
                container(contents)
                    .padding(9)
                    .width(Fill)
                    .style(move |theme| {
                        let mut style = card(theme);
                        if error.is_some() {
                            style.border.color = WARNING;
                        } else if selected {
                            style.border.color = ACCENT;
                        }
                        style
                    })
                    .boxed(),
            );
        }
        paths.push(text("Each path evaluates independently. Use the arrows to collapse cards; click a path's summary or edit its fields to select its plot handles.").size(11).color(MUTED).boxed()).boxed()
    }
    fn path_controls<'a>(&'a self, index: usize, path: &'a InputPath) -> Element<'a, Message> {
        let mode = self.project.settings.mode;
        let kind = path.kind(mode);
        let mut view = column![
            pick_list(
                Some(kind),
                if mode == Mode::Real {
                    &[InputKind::Interval, InputKind::Point][..]
                } else {
                    &[InputKind::Interval, InputKind::Curve, InputKind::Point][..]
                },
                ToString::to_string
            )
            .on_select(Message::PathKind)
            .width(Fill)
            .text_size(13)
        ]
        .spacing(10);
        if kind == InputKind::Interval {
            return view
                .push(
                    row![
                        self.path_setting(index, Field::Start, "Input start", &path.interval[0]),
                        self.path_setting(index, Field::End, "Input end", &path.interval[1])
                    ]
                    .spacing(8)
                    .boxed(),
                )
                .push(self.path_sampling_controls(index, path))
                .push(
                    text("Drag 1 / 2 handles along the real axis to edit the interval.")
                        .size(11)
                        .color(MUTED)
                        .boxed(),
                )
                .boxed();
        }
        if kind == InputKind::Curve {
            view = view
                .push(
                    pick_list(
                        Some(path.curve),
                        [Curve::Line, Curve::Bezier, Curve::Spline, Curve::Nurbs],
                        ToString::to_string,
                    )
                    .on_select(Message::Curve)
                    .text_size(13)
                    .boxed(),
                )
                .push(self.path_sampling_controls(index, path));
        }
        view = view.push(
            checkbox(self.active_path == index && self.draw_path)
                .label(if kind == InputKind::Point {
                    "Click plot to position the point"
                } else {
                    "Click plot to append points"
                })
                .text_size(12)
                .on_toggle(Message::DrawPath)
                .boxed(),
        );
        if mode == Mode::Real {
            view = view.push(
                text("Evaluates x; complex coordinates are retained when changing modes.")
                    .size(11)
                    .color(MUTED)
                    .boxed(),
            );
        }
        view = view.push(
            row![
                button(text("+ Add point").size(12))
                    .style(button::primary)
                    .on_press_maybe(
                        (path.control_points.len() < 64
                            && (kind != InputKind::Point || path.control_points.is_empty()))
                        .then_some(Message::AddControl)
                    ),
                button(text("Reset").size(12))
                    .style(button::text)
                    .on_press(Message::ResetPath),
                space().width(Fill),
                button(text("Clear all").size(12))
                    .style(button::danger)
                    .on_press(Message::ClearPath),
            ]
            .spacing(6)
            .boxed(),
        );
        for (i, point) in path
            .drafts()
            .into_iter()
            .take(if kind == InputKind::Point { 1 } else { 64 })
            .enumerate()
        {
            let [re, im] = point;
            let mut fields = row![
                text(format!("{}", i + 1)).size(12).width(18),
                self.input_field(
                    InputKey::Control(index, i, 0),
                    if mode == Mode::Real { "x" } else { "Re" },
                    re,
                    move |v| Message::ControlAxis(i, 0, v)
                ),
            ]
            .spacing(6)
            .align_y(iced::Center);
            if mode == Mode::Complex {
                fields = fields.push(self.input_field(
                    InputKey::Control(index, i, 1),
                    "Im",
                    im,
                    move |v| Message::ControlAxis(i, 1, v),
                ));
            }
            if kind == InputKind::Curve && path.curve == Curve::Nurbs {
                fields = fields.push(
                    container(self.input_field(
                        InputKey::ControlWeight(index, i),
                        "weight",
                        path.curve_weights.get(i).map_or("1", String::as_str),
                        move |v| Message::ControlWeight(i, v),
                    ))
                    .width(64)
                    .boxed(),
                );
            }
            view = view.push(
                fields
                    .push(
                        button(text("×").size(14))
                            .style(button::text)
                            .on_press(Message::Plot(Interaction::RemoveControl(i)))
                            .boxed(),
                    )
                    .boxed(),
            );
        }
        view.boxed()
    }
    fn path_setting<'a>(
        &'a self,
        path: usize,
        field: Field,
        label: &'static str,
        value: &'a str,
    ) -> Element<'a, Message> {
        column![
            text(label).size(11).color(MUTED),
            self.input_field(InputKey::PathSetting(path, field), label, value, move |v| {
                Message::Field(field, v)
            })
        ]
        .spacing(4)
        .width(Fill)
        .boxed()
    }
    fn path_sampling_controls<'a>(
        &'a self,
        index: usize,
        path: &'a InputPath,
    ) -> Element<'a, Message> {
        column![
            self.path_setting(
                index,
                Field::Samples,
                "Linear samples (16–100000)",
                &path.samples
            ),
            self.path_setting(
                index,
                Field::Extrapolation,
                "Extrapolation beyond endpoints (%)",
                &path.extrapolation
            )
        ]
        .spacing(8)
        .boxed()
    }
}
