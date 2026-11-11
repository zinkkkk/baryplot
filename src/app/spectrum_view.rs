use super::view::{ACCENT, MUTED, WARNING};
use super::*;
use crate::numerics::filter::{Action, Cleanup, Field as FilterField, Locations};
use iced::{
    Element, Fill, Widget,
    widget::{button, column, pick_list, row, space, text},
};
impl App {
    pub(super) fn spectrum_action(&mut self, action: Action) {
        let settings = &mut self.project.settings.spectrum;
        match action {
            Action::Locations(value) => settings.locations = value,
            Action::Cleanup(value) => settings.cleanup = value,
            Action::Text(field, value) => match field {
                FilterField::Tolerance => settings.tolerance = value,
                FilterField::MinPole => settings.min_pole = value,
                FilterField::MaxPole => settings.max_pole = value,
            },
            Action::Frame => {
                self.frame_spectrum();
                return;
            }
        }
        self.spectrum.error = None;
        self.visual_revision += 1;
    }
    pub(super) fn frame_spectrum(&mut self) {
        let Some(spectrum) = &self.spectrum.result else {
            return;
        };
        let filter = self.spectrum.filter;
        let points: Vec<_> = spectrum
            .poles
            .iter()
            .enumerate()
            .filter(|(i, _)| filter.pole(spectrum, *i))
            .map(|(_, z)| *z)
            .chain(
                spectrum
                    .zeros
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| filter.zero(spectrum, *i))
                    .map(|(_, z)| *z),
            )
            .collect();
        if points.is_empty() {
            self.spectrum.error = Some("No roots pass the current filters".into());
            return;
        }
        if points
            .iter()
            .any(|z| z.re.abs() >= 1e14 || z.im.abs() >= 1e14)
        {
            self.spectrum.error=Some("A root lies beyond the supported viewport; set a maximum pole magnitude before framing".into());
            return;
        }
        let points = points
            .iter()
            .copied()
            .chain(self.points.iter().map(|n| n.z));
        let (mut xmin, mut xmax, mut ymin, mut ymax) = (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        for z in points {
            xmin = xmin.min(z.re);
            xmax = xmax.max(z.re);
            ymin = ymin.min(z.im);
            ymax = ymax.max(z.im);
        }
        let mx = (xmax - xmin).max(1.) * 0.15;
        let my = (ymax - ymin).max(1.) * 0.15;
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
            self.rebuild();
            self.spectrum.error = None;
        }
    }
    pub(super) fn spectrum_view(&self) -> Element<'_, Message> {
        let settings = &self.project.settings.spectrum;
        let mut view=column![
            text("Poles, zeros and residues").size(18),
            text("Display filters and the numerical spectrum of the current manual setup or fitted preview. Calculations run after rendering and can be cancelled at any time.").size(12).color(MUTED),
            row![button(text("Calculate all").size(12)).on_press(Message::CalculateSpectrum).style(button::secondary),button(text("Frame poles/zeros").size(12)).on_press(Message::Spectrum(Action::Frame)).style(button::secondary),space().width(Fill),button(text("Plot settings").size(12)).on_press(Message::Tab(Tab::Plot)).style(button::text)].spacing(10),
            text("Locations").size(11).color(MUTED),
            pick_list(Some(settings.locations),[Locations::All,Locations::Real,Locations::Complex],ToString::to_string).on_select(|v|Message::Spectrum(Action::Locations(v))).text_size(13).width(Fill),
            row![self.spectrum_setting(FilterField::MinPole,"Minimum |pole|","no lower limit",&settings.min_pole),self.spectrum_setting(FilterField::MaxPole,"Maximum |pole|","no upper limit",&settings.max_pole)].spacing(10),
            text("Cleanup display filter").size(11).color(MUTED),
            pick_list(Some(settings.cleanup),[Cleanup::Off,Cleanup::Method1,Cleanup::Method2],ToString::to_string).on_select(|v|Message::Spectrum(Action::Cleanup(v))).text_size(12).width(Fill),
            self.spectrum_setting(FilterField::Tolerance,"Cleanup relative tolerance","1e-13",&settings.tolerance),
            text("Cleanup 1 uses residue size and distance to sample data. Cleanup 2 also uses pole–zero separation and domain tests. These filters hide flagged poles, their residues and nearest zero candidates; they do not change or refit the rational function.").size(11).color(MUTED),
        ].spacing(11);
        if self.project.settings.mode == Mode::Real {
            view=view.push(text("Complex locations are shown as (Re, Im) on the same axes as the real function plot. Poles use cross markers; zeros use rings. Choose Real only to hide complex roots.").size(11).color(MUTED).boxed());
        }
        if self.spectrum.busy || self.spectrum.pending.is_some() {
            view = view.push(
                row![
                    text("Calculating spectrum… plot remains live")
                        .size(12)
                        .color(ACCENT),
                    button(text("Cancel").size(12))
                        .on_press(Message::CancelSpectrum)
                        .style(button::danger)
                ]
                .spacing(10)
                .align_y(iced::Center)
                .boxed(),
            );
        } else if self.spectrum.paused {
            view = view.push(
                text("Spectrum cancelled. Use Calculate all or enable a display option to resume.")
                    .size(11)
                    .color(MUTED)
                    .boxed(),
            );
        }
        if let Some(error) = &self.spectrum.error {
            view = view.push(text(format!("! {error}")).size(12).color(WARNING).boxed());
        }
        if let Some(spectrum) = &self.spectrum.result {
            let filter = self.spectrum.filter;
            let poles = spectrum
                .poles
                .iter()
                .enumerate()
                .filter(|(i, _)| filter.pole(spectrum, *i))
                .count();
            let zeros = spectrum
                .zeros
                .iter()
                .enumerate()
                .filter(|(i, _)| filter.zero(spectrum, *i))
                .count();
            view = view
                .push(
                    text(format!(
                        "Showing {poles}/{} poles · {zeros}/{} zeros · {:.1} ms",
                        spectrum.poles.len(),
                        spectrum.zeros.len(),
                        spectrum.elapsed_ms
                    ))
                    .size(12)
                    .color(ACCENT)
                    .boxed(),
                )
                .push(text("Poles and residues").size(15).boxed());
            for (i, p) in spectrum
                .poles
                .iter()
                .enumerate()
                .filter(|(i, _)| filter.pole(spectrum, *i))
            {
                let residue = spectrum
                    .residues
                    .get(i)
                    .copied()
                    .flatten()
                    .map_or("unavailable".into(), model::number);
                view = view.push(
                    text(format!(
                        "p{}  {}\nresidue  {}   ·   |p| {:.6e}",
                        i + 1,
                        model::number(*p),
                        residue,
                        p.norm()
                    ))
                    .size(11)
                    .boxed(),
                );
            }
            if poles == 0 {
                view = view.push(
                    text("No finite poles pass the filters.")
                        .size(12)
                        .color(MUTED)
                        .boxed(),
                );
            }
            view = view.push(text("Zeros").size(15).boxed());
            for (i, z) in spectrum
                .zeros
                .iter()
                .enumerate()
                .filter(|(i, _)| filter.zero(spectrum, *i))
            {
                view = view.push(
                    text(format!("z{}  {}", i + 1, model::number(*z)))
                        .size(11)
                        .boxed(),
                );
            }
            if zeros == 0 {
                view = view.push(
                    text("No isolated zeros pass the filters.")
                        .size(12)
                        .color(MUTED)
                        .boxed(),
                );
            }
            for warning in &spectrum.warnings {
                view = view.push(text(warning).size(11).color(WARNING).boxed());
            }
        }
        view.push(space().height(12).boxed()).boxed()
    }
    fn spectrum_setting<'a>(
        &self,
        field: FilterField,
        label: &'static str,
        placeholder: &'a str,
        value: &'a str,
    ) -> Element<'a, Message> {
        column![
            text(label).size(11).color(MUTED),
            self.input_field(
                InputKey::SpectrumFilter(field),
                placeholder,
                value,
                move |v| Message::Spectrum(Action::Text(field, v))
            )
        ]
        .spacing(5)
        .width(Fill)
        .boxed()
    }
}
