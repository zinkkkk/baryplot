//! Shared HSV/RGB modal picker with live preview and reversible colour edits.
use crate::{
    app::{Message, appearance::Action},
    appearance::{Line, hsv, rgb_to_hsv},
};
use iced::{
    Color, Element, Point, Rectangle, Renderer, Size, Theme, Widget, mouse,
    widget::{
        self, button,
        canvas::{self, Frame, Geometry, Path, Stroke},
        column, container, row, slider, text,
    },
};
use std::cell::Cell;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Entry(usize),
    Line(Line),
    Guide(crate::axes::Guide),
    Background,
    Labels,
    CursorReadout,
}
#[derive(Debug, Clone, Copy)]
pub enum Edit {
    Hue(f32),
    SaturationValue(f32, f32),
    Channel(usize, u8),
    Swatch([f32; 3]),
}
pub struct Picker {
    pub target: Target,
    pub original: [f32; 3],
    hsv: [f32; 3],
}
impl Picker {
    pub fn new(target: Target, rgb: [f32; 3]) -> Self {
        Self {
            target,
            original: rgb,
            hsv: rgb_to_hsv(rgb),
        }
    }
    pub fn rgb(&self) -> [f32; 3] {
        hsv(self.hsv[0], self.hsv[1], self.hsv[2])
    }
    pub fn edit(&mut self, edit: Edit) {
        match edit {
            Edit::Hue(h) => self.hsv[0] = h,
            Edit::SaturationValue(s, v) => {
                self.hsv[1] = s.clamp(0., 1.);
                self.hsv[2] = v.clamp(0., 1.);
            }
            Edit::Channel(i, v) if i < 3 => {
                let mut rgb = self.rgb();
                rgb[i] = v as f32 / 255.;
                let mut converted = rgb_to_hsv(rgb);
                if converted[1] == 0. {
                    converted[0] = self.hsv[0];
                }
                self.hsv = converted;
            }
            Edit::Swatch(rgb) => self.hsv = rgb_to_hsv(rgb),
            _ => {}
        }
    }
    pub fn view(&self) -> Element<'_, Message> {
        let rgb = self.rgb();
        let mut body = column![
            row![
                text(match self.target {
                    Target::Entry(_) => "Data colour",
                    Target::Guide(_) => "Guide colour",
                    Target::Background => "Plot background colour",
                    Target::Labels => "Tick label colour",
                    Target::CursorReadout => "Cursor readout colour",
                    Target::Line(Line::Input) => "Input line colour",
                    Target::Line(Line::Output) => "Output line colour",
                })
                .size(16)
                .width(iced::Fill),
                button("×")
                    .on_press(Message::Appearance(Action::PickerCancel))
                    .style(button::text)
            ],
            row![
                swatch(self.original, 48., 28.),
                text("→"),
                swatch(rgb, 48., 28.),
                text(hex(rgb)).size(12)
            ]
            .spacing(9)
            .align_y(iced::Center),
            widget::canvas(SaturationSquare(self.hsv))
                .width(330)
                .height(190),
            row![
                text("Hue").size(12).width(40),
                slider(0.0..=360.0, self.hsv[0], |v| Message::Appearance(
                    Action::PickerEdit(Edit::Hue(v))
                ))
                .step(0.5)
            ],
        ]
        .spacing(10);
        for (i, label) in ["Red", "Green", "Blue"].into_iter().enumerate() {
            body = body.push(
                row![
                    text(label).size(12).width(40),
                    slider(0u8..=255u8, (rgb[i] * 255.).round() as u8, move |v| {
                        Message::Appearance(Action::PickerEdit(Edit::Channel(i, v)))
                    }),
                    text(format!("{:.0}", rgb[i] * 255.)).size(12).width(28)
                ]
                .spacing(7)
                .boxed(),
            );
        }
        let mut swatches = row![].spacing(4);
        for rgb in [
            [1., 1., 1.],
            [0., 0., 0.],
            [0.6, 0.6, 0.6],
            [1., 0.3, 0.35],
            [1., 0.65, 0.2],
            [1., 0.9, 0.2],
            [0.3, 0.85, 0.4],
            [0.3, 0.85, 0.85],
            [0.3, 0.55, 1.],
            [0.7, 0.4, 0.95],
        ] {
            swatches = swatches.push(
                button(swatch(rgb, 24., 22.))
                    .padding(2)
                    .on_press(Message::Appearance(Action::PickerEdit(Edit::Swatch(rgb))))
                    .boxed(),
            );
        }
        body = body.push(swatches.boxed()).push(
            row![
                widget::space().width(iced::Fill),
                button("Cancel")
                    .on_press(Message::Appearance(Action::PickerCancel))
                    .style(button::secondary),
                button("Use colour").on_press(Message::Appearance(Action::PickerAccept))
            ]
            .spacing(8)
            .boxed(),
        );
        container(body)
            .padding(16)
            .width(370)
            .style(container::bordered_box)
            .id("picker-dialog")
            .boxed()
    }
}
pub fn hex(rgb: [f32; 3]) -> String {
    format!(
        "#{:02X}{:02X}{:02X}",
        (rgb[0] * 255.).round() as u8,
        (rgb[1] * 255.).round() as u8,
        (rgb[2] * 255.).round() as u8
    )
}
pub fn swatch(rgb: [f32; 3], width: f32, height: f32) -> Element<'static, Message> {
    container(widget::space().width(width).height(height))
        .style(move |_| container::Style {
            background: Some(Color::from_rgb(rgb[0], rgb[1], rgb[2]).into()),
            border: iced::Border {
                width: 1.,
                color: Color::from_rgb(0.4, 0.45, 0.55),
                radius: 3.into(),
            },
            ..Default::default()
        })
        .boxed()
}
struct SaturationSquare([f32; 3]);
#[derive(Default)]
struct State {
    dragging: bool,
    cache: canvas::Cache,
    hue: Cell<Option<f32>>,
}
impl canvas::Program<Message> for SaturationSquare {
    type State = State;
    fn update(
        &self,
        state: &mut State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(bounds) =>
            {
                state.dragging = true
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.dragging = false;
                return None;
            }
            canvas::Event::Window(iced::window::Event::Unfocused) => {
                state.dragging = false;
                return None;
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) if state.dragging => {}
            _ => return None,
        }
        let p = cursor.position()?;
        Some(
            canvas::Action::publish(Message::Appearance(Action::PickerEdit(
                Edit::SaturationValue(
                    ((p.x - bounds.x) / bounds.width).clamp(0., 1.),
                    (1. - (p.y - bounds.y) / bounds.height).clamp(0., 1.),
                ),
            )))
            .and_capture(),
        )
    }
    fn mouse_interaction(
        &self,
        _: &State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
    fn draw(
        &self,
        state: &State,
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        if state.hue.get() != Some(self.0[0]) {
            state.cache.clear();
            state.hue.set(Some(self.0[0]));
        }
        let background = state.cache.draw(renderer, bounds.size(), |frame| {
            for y in 0..64 {
                for x in 0..96 {
                    let c = hsv(self.0[0], x as f32 / 95., 1. - y as f32 / 63.);
                    frame.fill_rectangle(
                        Point::new(
                            x as f32 * bounds.width / 96.,
                            y as f32 * bounds.height / 64.,
                        ),
                        Size::new(bounds.width / 96. + 0.5, bounds.height / 64. + 0.5),
                        Color::from_rgb(c[0], c[1], c[2]),
                    );
                }
            }
        });
        let mut marker = Frame::new(renderer, bounds.size());
        let p = Path::circle(
            Point::new(self.0[1] * bounds.width, (1. - self.0[2]) * bounds.height),
            5.,
        );
        marker.stroke(
            &p,
            Stroke::default().with_color(Color::BLACK).with_width(3.),
        );
        marker.stroke(
            &p,
            Stroke::default().with_color(Color::WHITE).with_width(1.),
        );
        vec![background, marker.into_geometry()]
    }
}
