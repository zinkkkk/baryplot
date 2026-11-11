use super::*;
use crate::axes::{Guide, LabelFormat, Scale};
use iced::widget::column;
#[derive(Debug, Clone)]
pub enum Action {
    Scale(bool, Scale),
    StrokeWidth(Guide, f32),
    StrokeOpacity(Guide, f32),
    StrokeDash(Guide, bool),
    Visible(Guide, bool),
    Marks(bool),
    Readout(bool),
    Ticks(bool, u16),
    Subdivisions(bool, u8),
    Labels(bool, bool),
    Stride(bool, u8),
    Format(LabelFormat),
    Precision(u8),
    Reset,
    ResetCircle,
    ResetBackground,
}
impl App {
    pub(super) fn guide_action(&mut self, a: Action) -> bool {
        let g = &mut self.project.settings.guides;
        let mut rebuild = false;
        match a {
            Action::Scale(x, scale) => {
                if x {
                    g.mapping.x = scale;
                } else {
                    g.mapping.y = scale;
                }
                self.project.settings.bounds = g.mapping.fit(self.project.settings.bounds);
                self.axis = axis_strings(self.project.settings.bounds);
                rebuild = true;
            }
            Action::StrokeWidth(t, v) => g.stroke_mut(t).width = v,
            Action::StrokeOpacity(t, v) => g.stroke_mut(t).opacity = v,
            Action::StrokeDash(t, v) => g.stroke_mut(t).dashed = v,
            Action::Visible(t, v) => match t {
                Guide::Circle => self.project.settings.unit_circle = v,
                Guide::Major => g.major = v,
                Guide::Minor => g.minor = v,
                Guide::Zero => g.zero = v,
                Guide::Border => g.border = v,
            },
            Action::Marks(v) => g.circle_marks = v,
            Action::Readout(v) => g.cursor_readout = v,
            Action::Ticks(x, v) => {
                if x {
                    g.x.ticks = v;
                } else {
                    g.y.ticks = v;
                }
            }
            Action::Subdivisions(x, v) => {
                if x {
                    g.x.subdivisions = v;
                } else {
                    g.y.subdivisions = v;
                }
            }
            Action::Labels(x, v) => {
                if x {
                    g.x.labels = v;
                } else {
                    g.y.labels = v;
                }
            }
            Action::Stride(x, v) => {
                if x {
                    g.x.label_stride = v;
                } else {
                    g.y.label_stride = v;
                }
            }
            Action::Format(v) => g.label_format = v,
            Action::Precision(v) => g.precision = v,
            Action::ResetCircle => {
                g.circle_style = crate::axes::Guides::default().circle_style;
                g.circle_marks = false;
            }
            Action::ResetBackground => {
                g.background = crate::axes::Guides::default().background;
            }
            Action::Reset => {
                *g = Default::default();
                self.project.settings.grid_step = "auto".into();
                self.project.settings.grid_y_step = "same".into();
                self.appearance_ui.drafts.clear();
                self.appearance_ui.errors.clear();
                rebuild = true;
            }
        }
        self.visual_revision += 1;
        rebuild
    }
    pub(super) fn guide_controls(&self, panel: Panel) -> Element<'_, Message> {
        let s = &self.project.settings;
        let g = s.guides;
        if panel == Panel::Circle {
            return column![
            text("Unit circle · |z| = 1").size(16),
            self.guide_stroke_controls(Guide::Circle,"Visible",s.unit_circle),
            checkbox(g.circle_marks).label("Mark the four cardinal points").text_size(12).on_toggle(|v|msg(Action::Marks(v))),
            text("The circle uses 2048 baked segments. Appearance changes update the guide without evaluating the function.").size(12).color(MUTED),
            button("Reset circle appearance").style(button::secondary).on_press(msg(Action::ResetCircle))
        ].spacing(16).boxed();
        }
        if panel == Panel::Background {
            return column![
            text("Plot background").size(16),
            self.guide_colour(picker::Target::Background,g.background),
            text("Click the swatch to choose a colour. This changes the plot and its axis margins.").size(12).color(MUTED),
            text("Function colouring blends over this background. Adjust Colour opacity to reveal more of the background.").size(12).color(MUTED),
            button("Reset background").style(button::secondary).on_press(msg(Action::ResetBackground)),
            text("Cursor readout").size(16),
            checkbox(g.cursor_readout).label("Show cursor coordinates and function value").text_size(12).on_toggle(|v|msg(Action::Readout(v))),
            self.guide_colour(picker::Target::CursorReadout,g.cursor_color),
            text("Shown at the plot's bottom left while the cursor is over the plot. In real mode, x and y are the cursor position; f(x) is the function value at x.").size(12).color(MUTED)
        ].spacing(16).boxed();
        }
        column![
            row![text("Grid and axes").size(16).width(Fill),self.toggle(Flag::Grid,"Grid visible",s.grid)].align_y(iced::Center),
            text("Drag either tick-label gutter to scale that axis. Log10 displays positive coordinates only; symmetric log includes zero and negatives.").size(11).color(MUTED),
            self.axis_controls(true),self.axis_controls(false),
            self.number_control(Number::Threshold,"Symmetric log: linear-region half width",g.mapping.threshold.to_string()),
            text("Spacing is in axis units: world units on linear axes, decades on log10, transformed units on symmetric log. 'auto' chooses a readable spacing; Y also accepts 'same'.").size(11).color(MUTED),
            row![self.number_control(Number::GridX,"X major spacing",s.grid_step.clone()),self.number_control(Number::GridY,"Y major spacing",s.grid_y_step.clone())].spacing(10),
            self.guide_stroke_controls(Guide::Major,"Major grid",g.major),
            self.guide_stroke_controls(Guide::Minor,"Minor grid",g.minor),
            self.guide_stroke_controls(Guide::Zero,"Zero axes",g.zero),
            self.guide_stroke_controls(Guide::Border,"Plot boundary",g.border),
            text("Tick labels").size(15),
            row![self.guide_colour(picker::Target::Labels,g.label_color),pick_list(Some(g.label_format),[LabelFormat::Auto,LabelFormat::Fixed,LabelFormat::Scientific],ToString::to_string).on_select(|v|msg(Action::Format(v)))].spacing(10).align_y(iced::Center),
            text(format!("Decimal precision · {}",g.precision)).size(12),slider(0u8..=8,g.precision,|v|msg(Action::Precision(v))),
            button("Reset all guide appearance").on_press(msg(Action::Reset)).style(button::secondary)
        ].spacing(14).boxed()
    }
    fn axis_controls(&self, x: bool) -> Element<'_, Message> {
        let g = self.project.settings.guides;
        let (a, scale) = if x {
            (g.x, g.mapping.x)
        } else {
            (g.y, g.mapping.y)
        };
        column![
            row![
                text(if x { "X axis" } else { "Y axis" }).size(14),
                pick_list(
                    Some(scale),
                    [Scale::Linear, Scale::Log10, Scale::SymLog],
                    ToString::to_string
                )
                .on_select(move |v| msg(Action::Scale(x, v))),
                checkbox(a.labels)
                    .label("Labels")
                    .text_size(12)
                    .on_toggle(move |v| msg(Action::Labels(x, v)))
            ]
            .spacing(10)
            .align_y(iced::Center),
            row![
                text(format!("Auto intervals · {}", a.ticks))
                    .size(12)
                    .width(150),
                slider(2u16..=24, a.ticks, move |v| msg(Action::Ticks(x, v)))
            ]
            .spacing(10),
            row![
                text(format!("Subdivisions · {}", a.subdivisions))
                    .size(12)
                    .width(150),
                slider(1u8..=10, a.subdivisions, move |v| msg(
                    Action::Subdivisions(x, v)
                ))
            ]
            .spacing(10),
            row![
                text(format!("Label every {} tick(s)", a.label_stride))
                    .size(12)
                    .width(150),
                slider(1u8..=10, a.label_stride, move |v| msg(Action::Stride(x, v)))
            ]
            .spacing(10)
        ]
        .spacing(7)
        .boxed()
    }
    fn guide_colour(&self, target: picker::Target, rgb: [f32; 3]) -> Element<'_, Message> {
        row![
            button(picker::swatch(rgb, 54., 18.))
                .padding(3)
                .style(button::secondary)
                .on_press(Message::Appearance(super::Action::Pick(target))),
            text(picker::hex(rgb)).size(11).color(MUTED)
        ]
        .spacing(8)
        .align_y(iced::Center)
        .boxed()
    }
    fn guide_stroke_controls(
        &self,
        t: Guide,
        title: &'static str,
        enabled: bool,
    ) -> Element<'_, Message> {
        let s = self.project.settings.guides.stroke(t);
        column![
            row![
                checkbox(enabled)
                    .label(title)
                    .text_size(13)
                    .on_toggle(move |v| msg(Action::Visible(t, v))),
                space().width(Fill),
                self.guide_colour(picker::Target::Guide(t), s.color),
                checkbox(s.dashed)
                    .label("Dashed")
                    .text_size(12)
                    .on_toggle(move |v| msg(Action::StrokeDash(t, v)))
            ]
            .spacing(10)
            .align_y(iced::Center),
            row![
                text(format!("{:.1} px", s.width)).size(11).width(45),
                slider(0.2..=6., s.width, move |v| msg(Action::StrokeWidth(t, v))).step(0.1),
                text(format!("{:.0}%", s.opacity * 100.)).size(11).width(35),
                slider(0.0..=1., s.opacity, move |v| msg(Action::StrokeOpacity(
                    t, v
                )))
                .step(0.01)
            ]
            .spacing(9)
            .align_y(iced::Center)
        ]
        .spacing(6)
        .boxed()
    }
}
fn msg(a: Action) -> Message {
    Message::Appearance(super::Action::Guide(a))
}
