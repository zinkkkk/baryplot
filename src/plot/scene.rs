//! Cached grid/vector layers and the transient cursor overlay.
use super::*;

impl Plot<'_> {
    pub(super) fn draw_scene(
        &self,
        state: &State,
        renderer: &Renderer,
        bounds: Rectangle,
    ) -> Vec<Geometry> {
        state
            .pixel_scale
            .set(Some(renderer.scale().unwrap_or_default().total()));
        if state.key.get() != self.revision {
            state.cache.clear();
            state.key.set(self.revision);
        }
        let a = area(bounds.size());
        let b = Projection::new(
            self.project.settings.bounds,
            self.project.settings.guides.mapping,
        );
        let s = &self.project.settings;
        let axes_key = AxesKey {
            bounds: b.bounds,
            grid_y_step: self.plan.and_then(|p| p.grid_y_step),
            guides: s.guides,
            grid_step: self.plan.and_then(|p| p.grid_step),
            mode: s.mode,
            grid: s.grid,
        };
        if state.axes_key.get() != Some(axes_key) {
            state.axes_cache.clear();
            state.grid_cache.clear();
            state.axes_key.set(Some(axes_key));
        }
        let grid = state.grid_cache.draw_with_bounds(renderer, a, |frame| {
            if s.grid {
                draw_grid(frame, b, a, self.plan, s.guides);
            }
        });
        let plot = state.cache.draw_with_bounds(renderer, a, |frame| {
            let local = a;
            draw_paths(frame, s, self.plan, self.data, b, local);
            if let Some(spectrum) = self.spectrum {
                draw_spectrum(frame, spectrum, self.spectrum_filter, s, b, local);
            }
            let markers = self.displayed_markers(b, local);
            // Canvas text is layered above vector fills. Keep only the front label
            // at coincident centres so an interval handle can cover a node cleanly.
            let mut positions = std::collections::HashSet::new();
            let mut labels = vec![false; markers.len()];
            for (i, marker) in markers.iter().enumerate().rev() {
                labels[i] = positions.insert((marker.point.x.to_bits(), marker.point.y.to_bits()));
            }
            for (marker, show_label) in markers.into_iter().zip(labels) {
                let Marker {
                    target,
                    point,
                    color: c,
                    letter,
                } = marker;
                if self
                    .highlighted_entry
                    .is_some_and(|i| target_row(target) == Some(i))
                {
                    frame.stroke(
                        &Path::circle(point, 11.5),
                        Stroke::default()
                            .with_width(4.)
                            .with_color(color(s.guides.background)),
                    );
                    frame.stroke(
                        &Path::circle(point, 11.5),
                        Stroke::default().with_width(2.).with_color(c),
                    );
                }
                let shape = Path::circle(point, 8.);
                let derived = matches!(target, Target::Derived(_));
                frame.fill(
                    &shape,
                    if derived {
                        color(s.guides.background)
                    } else {
                        c
                    },
                );
                frame.stroke(
                    &shape,
                    Stroke::default().with_width(1.4).with_color(if derived {
                        c
                    } else {
                        color(s.guides.background)
                    }),
                );
                if !show_label {
                    continue;
                }
                frame.fill_text(canvas::Text {
                    content: letter,
                    position: point,
                    color: if derived {
                        c
                    } else if c.r * 0.2126 + c.g * 0.7152 + c.b * 0.0722 > 0.45 {
                        BG
                    } else {
                        Color::WHITE
                    },
                    size: 11.0.into(),
                    font: iced::Font {
                        weight: iced::font::Weight::Bold,
                        ..Default::default()
                    },
                    align_x: iced::advanced::text::Alignment::Center,
                    align_y: iced::alignment::Vertical::Center,
                    ..Default::default()
                });
            }
        });
        let axes = state.axes_cache.draw(renderer, bounds.size(), |frame| {
            let g = s.guides;
            let ink = color(g.label_color);
            if g.border {
                frame.stroke_rectangle(
                    Point::new(a.x, a.y),
                    a.size(),
                    guide_stroke(g.border_style),
                );
            }
            let projected = b.mapping.projected(b.bounds);
            if g.x.labels {
                for (i, x) in axes::ticks(
                    projected.x_min,
                    projected.x_max,
                    self.plan.and_then(|p| p.grid_step),
                    g.x.ticks,
                )
                .into_iter()
                .enumerate()
                {
                    if i % usize::from(g.x.label_stride.max(1)) != 0 {
                        continue;
                    }
                    let value = b.mapping.x.inverse(x, b.mapping.threshold);
                    let p = screen(C::new(value, b.y_min), b, a);
                    label(
                        frame,
                        g.label(value),
                        Point::new(p.x - 10., a.y + a.height + 9.),
                        ink,
                        11.,
                    );
                }
                label(
                    frame,
                    if s.mode == Mode::Real { "x" } else { "Re" }.into(),
                    Point::new(a.x + a.width - 15., a.y + a.height + 23.),
                    ink,
                    11.,
                );
            }
            if g.y.labels {
                for (i, y) in axes::ticks(
                    projected.y_min,
                    projected.y_max,
                    self.plan.and_then(|p| p.grid_y_step),
                    g.y.ticks,
                )
                .into_iter()
                .enumerate()
                {
                    if i % usize::from(g.y.label_stride.max(1)) != 0 {
                        continue;
                    }
                    let value = b.mapping.y.inverse(y, b.mapping.threshold);
                    let p = screen(C::new(b.x_min, value), b, a);
                    label(frame, g.label(value), Point::new(4., p.y - 7.), ink, 11.);
                }
                label(
                    frame,
                    if s.mode == Mode::Real { "f(x)" } else { "Im" }.into(),
                    Point::new(9., a.y - 18.),
                    ink,
                    11.,
                );
            }
        });
        let mut geometry = vec![grid];
        if s.unit_circle {
            if state.circle_key.get() != Some(axes_key) {
                state.circle_cache.clear();
                state.circle_key.set(Some(axes_key));
            }
            let circle = state.circle_cache.draw_with_bounds(renderer, a, |frame| {
                let path = Path::new(|builder| {
                    let mut connected = false;
                    for &[x, y] in &UNIT_CIRCLE {
                        let p = screen(C::new(x, y), b, a);
                        if !finite(p) {
                            connected = false;
                            continue;
                        }
                        if connected {
                            builder.line_to(p);
                        } else {
                            builder.move_to(p);
                        }
                        connected = true;
                    }
                });
                frame.stroke(&path, guide_stroke(s.guides.circle_style));
                if s.guides.circle_marks {
                    for z in [
                        C::new(1., 0.),
                        C::new(-1., 0.),
                        C::new(0., 1.),
                        C::new(0., -1.),
                    ] {
                        let p = screen(z, b, a);
                        if finite(p) && a.contains(p) {
                            frame.fill(
                                &Path::circle(p, 3.),
                                alpha(
                                    color(s.guides.circle_style.color),
                                    s.guides.circle_style.opacity,
                                ),
                            );
                        }
                    }
                }
            });
            geometry.push(circle);
        }
        geometry.extend([plot, axes]);
        if let Some(p) = state.hover.filter(|p| a.contains(*p)) {
            let mut frame = Frame::new(renderer, bounds.size());
            let hit = self.hit(p, b, a);
            if hit.is_some() {
                frame.stroke(
                    &Path::circle(p, 11.),
                    Stroke::default()
                        .with_width(1.)
                        .with_color(alpha(OUTPUT, 0.8)),
                );
            }
            if s.guides.cursor_readout {
                let z = world(p, b, a);
                let value = self.model.and_then(|m| {
                    m.evaluate(if s.mode == Mode::Real {
                        C::new(z.re, 0.)
                    } else {
                        z
                    })
                });
                label(
                    &mut frame,
                    cursor_caption(s.mode, z, value),
                    Point::new(a.x + 8., a.y + a.height - 22.),
                    color(s.guides.cursor_color),
                    12.,
                );
            }
            geometry.push(frame.into_geometry());
        }
        geometry
    }
}
