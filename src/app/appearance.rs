pub(crate) mod guides;
use super::view::{MUTED, WARNING};
use super::*;
use crate::{
    appearance::{self as style, Contours, Line, ModulusScale, Palette},
    picker::{self, Picker},
};
use iced::{
    Element, Fill, Widget,
    widget::{
        self, button, checkbox, column, container, pick_list, row, scrollable, slider, space, text,
        text_input,
    },
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Lines,
    Colour,
    Guides,
    Circle,
    Background,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Number {
    Resolution,
    PhaseStep,
    PhaseOffset,
    ModulusSpacing,
    GridX,
    GridY,
    Threshold,
    BandBase,
    BandExponent,
}
#[derive(Debug, Clone)]
pub enum Action {
    Open(Panel),
    Guide(guides::Action),
    Close,
    Number(Number, String),
    Resize(bool),
    Classic,
    Modern,
    Saturation(f32),
    ShadingFloor(f32),
    Bands(bool),
    BandFloor(f32),
    BandStrength(f32),
    Width(Line, f32),
    Opacity(Line, f32),
    Dashed(Line, bool),
    Extrapolation(Line, bool),
    Polygon(bool),
    Palette(Palette),
    Reverse(bool),
    Shading(bool),
    Contours(Contours),
    ModulusScale(ModulusScale),
    Strength(f32),
    ContourWidth(f32),
    Pick(picker::Target),
    PickerEdit(picker::Edit),
    PickerCancel,
    PickerAccept,
}
#[derive(Default)]
pub struct Ui {
    pub panel: Option<Panel>,
    pub picker: Option<Picker>,
    pub(super) drafts: std::collections::BTreeMap<Number, String>,
    pub(super) errors: std::collections::BTreeMap<Number, String>,
}
impl App {
    pub(super) fn appearance_action(&mut self, action: Action) -> bool {
        use Action::*;
        match action {
            Guide(a) => self.guide_action(a),
            Open(panel) => {
                self.appearance_ui.panel = Some(panel);
                self.appearance_ui.drafts.clear();
                self.appearance_ui.errors.clear();
                false
            }
            Close => {
                self.appearance_ui.panel = None;
                false
            }
            Pick(target) => {
                let rgb = match target {
                    picker::Target::Entry(i) => self
                        .project
                        .entries
                        .get(i)
                        .and_then(|e| model::parse_color(&e.color))
                        .unwrap_or([1.; 3]),
                    picker::Target::Guide(t) => self.project.settings.guides.stroke(t).color,
                    picker::Target::Background => self.project.settings.guides.background,
                    picker::Target::Labels => self.project.settings.guides.label_color,
                    picker::Target::CursorReadout => self.project.settings.guides.cursor_color,
                    picker::Target::Line(Line::Input) => self.project.settings.input_line.color,
                    picker::Target::Line(Line::Output) => self.project.settings.output_line.color,
                };
                self.appearance_ui.picker = Some(Picker::new(target, rgb));
                false
            }
            PickerEdit(edit) => {
                if let Some(p) = &mut self.appearance_ui.picker {
                    p.edit(edit);
                    let target = p.target;
                    let rgb = p.rgb();
                    self.apply_colour(target, rgb);
                }
                false
            }
            PickerCancel => {
                if let Some(p) = self.appearance_ui.picker.take() {
                    self.apply_colour(p.target, p.original);
                }
                false
            }
            PickerAccept => {
                self.appearance_ui.picker = None;
                false
            }
            Number(field, value) => {
                self.appearance_ui.drafts.insert(field, value.clone());
                if matches!(field, self::Number::GridX | self::Number::GridY) {
                    let keyword = value.trim();
                    let valid = keyword == "auto"
                        || (field == self::Number::GridY && keyword == "same")
                        || crate::expression::evaluate(
                            &value,
                            &self.project.resolve_definitions().variables,
                        )
                        .is_ok_and(|z| z.im == 0. && z.re > 0.);
                    if !valid {
                        self.appearance_ui.errors.insert(
                            field,
                            "Use a positive expression, 'auto', or 'same' for Y".into(),
                        );
                        return false;
                    }
                    if field == self::Number::GridX {
                        self.project.settings.grid_step = value;
                    } else {
                        self.project.settings.grid_y_step = value;
                    }
                    self.appearance_ui.errors.remove(&field);
                    return true;
                }
                let variables = self.project.resolve_definitions().variables;
                let parsed = crate::expression::evaluate(&value, &variables).and_then(|z| {
                    if z.im == 0. {
                        Ok(z.re)
                    } else {
                        Err("Expected a real value".into())
                    }
                });
                let result = parsed.and_then(|v| {
                    match field {
                        self::Number::Threshold => {
                            if !(1e-12..=1e12).contains(&v) {
                                return Err("Use a linear half width from 1e-12 to 1e12".into());
                            }
                            self.project.settings.guides.mapping.threshold = v;
                        }
                        self::Number::Resolution => {
                            if v.fract()!=0. || v<1. || v>style::MAX_RESOLUTION as f64 {
                                return Err(format!("Use an integer pixel width from 1 to {}",style::MAX_RESOLUTION));
                            }
                            let s=&self.project.settings;
                            let domain=self.plan.as_ref().map_or(s.bounds,|p|p.domain);
                            let domain=s.guides.mapping.projected(domain);
                            style::image_size(v as u32,domain.height()/domain.width())?;
                            self.project.settings.resolution=v as u32;
                        }
                        _ => {
                            let mut next=self.project.settings.colour_style;
                            match field {
                                self::Number::PhaseStep=>next.phase_step=v,
                                self::Number::PhaseOffset=>next.phase_offset=v,
                                self::Number::ModulusSpacing=>next.modulus_spacing=v,
                                self::Number::BandBase=>next.band_base=v,
                                self::Number::BandExponent=>next.band_exponent=v,
                                _=>{}
                            }
                            if !next.valid() {
                                return Err(match field {
                                    self::Number::PhaseStep=>"Phase interval must be > 0 and ≤ 360 degrees",
                                    self::Number::ModulusSpacing=>"Use a base greater than 1 for log contours, or a positive linear interval",
                                    self::Number::BandBase=>"Brightness band base must be finite and greater than 1",
                                    self::Number::BandExponent=>"Band ramp exponent must be from 0.05 to 8",
                                    _=>"Expected a finite value"
                                }.into());
                            }
                            self.project.settings.colour_style=next;
                        }
                    }
                    Ok(())
                });
                match result {
                    Ok(()) => {
                        self.appearance_ui.errors.remove(&field);
                        self.latest.fetch_add(1, Ordering::Relaxed);
                        true
                    }
                    Err(e) => {
                        self.appearance_ui.errors.insert(field, e);
                        false
                    }
                }
            }
            Modern => {
                self.project.settings.colour_style = Default::default();
                self.appearance_ui.drafts.clear();
                self.appearance_ui.errors.clear();
                true
            }
            Saturation(v) => {
                self.project.settings.colour_style.saturation = f64::from(v);
                true
            }
            ShadingFloor(v) => {
                self.project.settings.colour_style.shading_floor = f64::from(v);
                true
            }
            Bands(v) => {
                self.project.settings.colour_style.bands = v;
                true
            }
            BandFloor(v) => {
                let c = &mut self.project.settings.colour_style;
                c.band_floor = f64::from(v);
                c.band_strength = c.band_strength.min(1. - c.band_floor);
                true
            }
            BandStrength(v) => {
                let c = &mut self.project.settings.colour_style;
                c.band_strength = f64::from(v).min(1. - c.band_floor);
                true
            }
            Classic => {
                self.project.settings.colour_style = style::ColourStyle::classic();
                self.project.settings.color_opacity = 0.75;
                self.appearance_ui.drafts.clear();
                self.appearance_ui.errors.clear();
                true
            }
            Resize(double) => {
                let current = self.project.settings.resolution;
                let next = if double {
                    current.saturating_mul(2).min(style::MAX_RESOLUTION)
                } else {
                    (current / 2).max(1)
                };
                self.appearance_action(Number(self::Number::Resolution, next.to_string()))
            }
            Width(line, v) => {
                self.line_style_mut(line).width = v;
                self.visual_revision += 1;
                false
            }
            Opacity(Line::Input, v) => {
                self.project.settings.input_opacity = v;
                self.visual_revision += 1;
                false
            }
            Opacity(Line::Output, v) => {
                self.project.settings.line_opacity = v;
                self.visual_revision += 1;
                false
            }
            Dashed(line, v) => {
                self.line_style_mut(line).dashed = v;
                self.visual_revision += 1;
                false
            }
            Extrapolation(line, v) => {
                self.line_style_mut(line).extrapolation = v;
                self.visual_revision += 1;
                false
            }
            Polygon(v) => {
                self.project.settings.input_line.control_polygon = v;
                self.visual_revision += 1;
                false
            }
            Palette(v) => {
                self.project.settings.colour_style.palette = v;
                true
            }
            Reverse(v) => {
                self.project.settings.colour_style.reverse = v;
                true
            }
            Shading(v) => {
                self.project.settings.colour_style.shading = v;
                true
            }
            Contours(v) => {
                self.project.settings.colour_style.contours = v;
                true
            }
            ModulusScale(v) => {
                let s = &mut self.project.settings.colour_style;
                s.modulus_scale = v;
                if v == style::ModulusScale::Logarithmic && s.modulus_spacing <= 1. {
                    s.modulus_spacing = 2.;
                }
                self.appearance_ui
                    .drafts
                    .remove(&self::Number::ModulusSpacing);
                self.appearance_ui
                    .errors
                    .remove(&self::Number::ModulusSpacing);
                true
            }
            Strength(v) => {
                self.project.settings.colour_style.contour_strength = v;
                true
            }
            ContourWidth(v) => {
                self.project.settings.colour_style.contour_width = v;
                true
            }
        }
    }
    fn line_style_mut(&mut self, line: Line) -> &mut style::LineStyle {
        match line {
            Line::Input => &mut self.project.settings.input_line,
            Line::Output => &mut self.project.settings.output_line,
        }
    }
    fn apply_colour(&mut self, target: picker::Target, rgb: [f32; 3]) {
        match target {
            picker::Target::Entry(i) => {
                if let Some(e) = self.project.entries.get_mut(i) {
                    e.color = picker::hex(rgb);
                }
            }
            picker::Target::Line(line) => self.line_style_mut(line).color = rgb,
            picker::Target::Guide(t) => self.project.settings.guides.stroke_mut(t).color = rgb,
            picker::Target::Background => self.project.settings.guides.background = rgb,
            picker::Target::Labels => self.project.settings.guides.label_color = rgb,
            picker::Target::CursorReadout => self.project.settings.guides.cursor_color = rgb,
        };
        self.visual_revision += 1;
    }
    pub(super) fn appearance_dialog(&self, panel: Panel) -> Element<'_, Message> {
        let body = match panel {
            Panel::Lines => column![
                self.line_controls(Line::Input),
                self.line_controls(Line::Output)
            ]
            .spacing(20)
            .boxed(),
            Panel::Colour => self.colour_controls(),
            Panel::Guides | Panel::Circle | Panel::Background => self.guide_controls(panel),
        };
        let mut tabs = row![].spacing(7);
        if matches!(panel, Panel::Guides | Panel::Circle | Panel::Background) {
            for (target, label) in [
                (Panel::Guides, "Grid and axes"),
                (Panel::Circle, "Unit circle"),
                (Panel::Background, "Background / readout"),
            ] {
                tabs = tabs.push(
                    button(text(label).size(12))
                        .padding([6, 10])
                        .style(if target == panel {
                            button::primary
                        } else {
                            button::secondary
                        })
                        .on_press(Message::Appearance(Action::Open(target)))
                        .boxed(),
                );
            }
        }
        container(
            column![
                row![
                    text(match panel {
                        Panel::Lines => "Input / output display",
                        Panel::Colour => "Function colouring",
                        Panel::Guides | Panel::Circle | Panel::Background => "Plot appearance",
                    })
                    .size(18)
                    .width(Fill),
                    button("×")
                        .on_press(Message::Appearance(Action::Close))
                        .style(button::text)
                ],
                tabs,
                scrollable(body).spacing(10).height(Fill)
            ]
            .spacing(12),
        )
        .padding(18)
        .width(580)
        .height(610)
        .style(container::bordered_box)
        .id("settings-dialog")
        .boxed()
    }
    fn line_controls(&self, line: Line) -> Element<'_, Message> {
        let s = &self.project.settings;
        let st = match line {
            Line::Input => s.input_line,
            Line::Output => s.output_line,
        };
        let opacity = if line == Line::Input {
            s.input_opacity
        } else {
            s.line_opacity
        };
        let mut controls = column![
            row![
                text(if line == Line::Input {
                    "Input line"
                } else {
                    "Output line"
                })
                .size(15)
                .width(Fill),
                self.toggle(
                    if line == Line::Input {
                        Flag::InputPath
                    } else {
                        Flag::Output
                    },
                    "Visible",
                    if line == Line::Input {
                        s.input_path
                    } else {
                        s.output
                    }
                )
            ]
            .align_y(iced::Center),
            row![
                text("Colour").size(12),
                button(picker::swatch(st.color, 72., 22.))
                    .padding(3)
                    .on_press(Message::Appearance(Action::Pick(picker::Target::Line(
                        line
                    ))))
                    .style(button::secondary),
                text(picker::hex(st.color)).size(11).color(MUTED)
            ]
            .spacing(8)
            .align_y(iced::Center),
            text(format!("Thickness · {:.1} px", st.width)).size(12),
            slider(0.2..=12., st.width, move |v| Message::Appearance(
                Action::Width(line, v)
            ))
            .step(0.1),
            text(format!("Opacity · {:.0}%", opacity * 100.)).size(12),
            slider(0.0..=1.0, opacity, move |v| Message::Appearance(
                Action::Opacity(line, v)
            ))
            .step(0.01),
            row![
                checkbox(st.dashed)
                    .label("Dashed")
                    .text_size(12)
                    .on_toggle(move |v| Message::Appearance(Action::Dashed(line, v))),
                checkbox(st.extrapolation)
                    .label("Show extrapolation")
                    .text_size(12)
                    .on_toggle(move |v| Message::Appearance(Action::Extrapolation(line, v)))
            ]
            .spacing(18)
        ]
        .spacing(8);
        if line == Line::Input {
            controls = controls.push(
                checkbox(st.control_polygon)
                    .label("Show control polygon (complex paths)")
                    .text_size(12)
                    .on_toggle(|v| Message::Appearance(Action::Polygon(v)))
                    .boxed(),
            );
        }
        controls.boxed()
    }
    pub(super) fn resolution_control(&self) -> Element<'_, Message> {
        row![
            self.number_control(
                Number::Resolution,
                "Pixel width",
                self.project.settings.resolution.to_string()
            ),
            button("½")
                .on_press(Message::Appearance(Action::Resize(false)))
                .style(button::secondary),
            button("2×")
                .on_press(Message::Appearance(Action::Resize(true)))
                .style(button::secondary)
        ]
        .spacing(8)
        .align_y(iced::Bottom)
        .boxed()
    }
    fn number_control(
        &self,
        field: Number,
        label: &'static str,
        current: String,
    ) -> Element<'_, Message> {
        let value = self
            .appearance_ui
            .drafts
            .get(&field)
            .cloned()
            .unwrap_or(current);
        let error = self.appearance_ui.errors.get(&field).cloned();
        let invalid = error.is_some();
        let input = text_input(label, value)
            .on_input(move |v| Message::Appearance(Action::Number(field, v)))
            .size(13)
            .padding(7)
            .style(move |theme, status| {
                let mut s = text_input::default(theme, status);
                if invalid {
                    s.border.color = WARNING;
                }
                s
            });
        let mut col = column![text(label).size(11).color(MUTED), input]
            .spacing(4)
            .width(Fill);
        if let Some(error) = error {
            col = col.push(text(error).size(11).color(WARNING).boxed());
        }
        col.boxed()
    }
    fn colour_controls(&self) -> Element<'_, Message> {
        let s = &self.project.settings;
        let c = s.colour_style;
        column![
            row![button(text("Restore original colouring").size(12)).on_press(Message::Appearance(Action::Classic)).style(button::secondary),button(text("Contour defaults").size(12)).on_press(Message::Appearance(Action::Modern)).style(button::secondary)].spacing(10),
            text("Original restores softer saturation, magnitude bands repeating at powers of two, smooth shading and 75% opacity. All settings remain editable.").size(11).color(MUTED),
            self.resolution_control(),
            text("Pixel height follows the viewport. Pixel width can be any integer from 1–16384 within the 256 MiB image budget.").size(11).color(MUTED),
            text("Palette").size(15),
            pick_list(Some(c.palette),Palette::ALL,ToString::to_string).on_select(|v|Message::Appearance(Action::Palette(v))).width(Fill),
            widget::canvas(Gradient(c)).width(Fill).height(22),
            checkbox(c.reverse).label("Reverse palette").text_size(12).on_toggle(|v|Message::Appearance(Action::Reverse(v))),
            text(format!("Saturation · {:.0}%",c.saturation*100.)).size(12),slider(0.0..=1.,c.saturation as f32,|v|Message::Appearance(Action::Saturation(v))).step(0.01),
            self.number_control(Number::PhaseOffset,"Phase origin (degrees)",c.phase_offset.to_string()),
            text("Magnitude shading").size(15),
            checkbox(c.shading).label("Smooth magnitude shading").text_size(12).on_toggle(|v|Message::Appearance(Action::Shading(v))),
            text(format!("Near-zero brightness · {:.0}%",c.shading_floor*100.)).size(12),slider(0.0..=1.,c.shading_floor as f32,|v|Message::Appearance(Action::ShadingFloor(v))).step(0.01),
            self.band_controls(),
            self.contour_controls(),
            text(format!("Colour opacity · {:.0}%",s.color_opacity*100.)).size(12),slider(0.0..=1.,s.color_opacity,Message::ColorOpacity).step(0.01),
            text("Colour domain").size(15),pick_list(Some(s.domain_extent),[Extent::Percent,Extent::Absolute],ToString::to_string).on_select(Message::Extent),
            row![self.setting(Field::MarginX,"Horizontal margin",&s.domain_margin[0]),self.setting(Field::MarginY,"Vertical margin",&s.domain_margin[1])].spacing(10),space().height(4)
        ].spacing(10).boxed()
    }
    fn band_controls(&self) -> Element<'_, Message> {
        let c = self.project.settings.colour_style;
        column![
            text("Brightness bands").size(15),
            checkbox(c.bands).label("Repeat brightness with magnitude").text_size(12).on_toggle(|v|Message::Appearance(Action::Bands(v))),
            text("Independent of contour lines. Base 2 repeats from |f| to 2|f|; a ramp exponent below 1 gives softer highlights.").size(11).color(MUTED),
            row![self.number_control(Number::BandBase,"Band magnitude base",c.band_base.to_string()),self.number_control(Number::BandExponent,"Band ramp exponent",c.band_exponent.to_string())].spacing(10),
            text(format!("Band minimum brightness · {:.0}%",c.band_floor*100.)).size(12),slider(0.0..=1.,c.band_floor as f32,|v|Message::Appearance(Action::BandFloor(v))).step(0.01),
            text(format!("Band brightness increase · {:.0}% (peak {:.0}%)",c.band_strength*100.,(c.band_floor+c.band_strength)*100.)).size(12),slider(0.0..=1.,c.band_strength as f32,|v|Message::Appearance(Action::BandStrength(v))).step(0.01),
        ].spacing(10).boxed()
    }
    fn contour_controls(&self) -> Element<'_, Message> {
        let c = self.project.settings.colour_style;
        column![
            text("Contour lines").size(15),
            pick_list(
                Some(c.contours),
                [
                    Contours::Off,
                    Contours::Phase,
                    Contours::Modulus,
                    Contours::Both
                ],
                ToString::to_string
            )
            .on_select(|v| Message::Appearance(Action::Contours(v))),
            self.number_control(
                Number::PhaseStep,
                "Phase interval (degrees)",
                c.phase_step.to_string()
            ),
            row![
                pick_list(
                    Some(c.modulus_scale),
                    [ModulusScale::Logarithmic, ModulusScale::Linear],
                    ToString::to_string
                )
                .on_select(|v| Message::Appearance(Action::ModulusScale(v))),
                self.number_control(
                    Number::ModulusSpacing,
                    if c.modulus_scale == ModulusScale::Logarithmic {
                        "Modulus base"
                    } else {
                        "Modulus interval"
                    },
                    c.modulus_spacing.to_string()
                )
            ]
            .spacing(10)
            .align_y(iced::Bottom),
            text(format!(
                "Contour contrast · {:.0}%",
                c.contour_strength * 100.
            ))
            .size(12),
            slider(0.0..=1., c.contour_strength, |v| Message::Appearance(
                Action::Strength(v)
            ))
            .step(0.01),
            text("Contour width").size(12),
            slider(0.001..=0.2, c.contour_width, |v| Message::Appearance(
                Action::ContourWidth(v)
            ))
            .step(0.001),
        ]
        .spacing(10)
        .boxed()
    }
}
struct Gradient(style::ColourStyle);
impl widget::canvas::Program<Message> for Gradient {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &iced::Renderer,
        _: &iced::Theme,
        bounds: iced::Rectangle,
        _: iced::mouse::Cursor,
    ) -> Vec<widget::canvas::Geometry> {
        let mut frame = widget::canvas::Frame::new(renderer, bounds.size());
        for i in 0..192 {
            let t = i as f64 / 191.;
            let rgb = self
                .0
                .palette_colour(if self.0.reverse { 1. - t } else { t });
            frame.fill_rectangle(
                iced::Point::new(i as f32 * bounds.width / 192., 0.),
                iced::Size::new(bounds.width / 192. + 0.5, bounds.height),
                iced::Color::from_rgb(rgb[0], rgb[1], rgb[2]),
            );
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests;
