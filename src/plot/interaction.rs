//! Pointer gestures, axis scaling, zoom and drag noise handling.
use super::*;

impl Plot<'_> {
    pub(super) fn update_canvas(
        &self,
        state: &mut State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Interaction>> {
        let a = area(bounds.size());
        let b = Projection::new(
            self.project.settings.bounds,
            self.project.settings.guides.mapping,
        );
        let position = cursor.position_in(bounds);
        match event {
            canvas::Event::Keyboard(
                iced::keyboard::Event::ModifiersChanged(modifiers)
                | iced::keyboard::Event::KeyPressed { modifiers, .. }
                | iced::keyboard::Event::KeyReleased { modifiers, .. },
            ) => {
                state.modifiers = *modifiers;
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(_)) => {
                if state.drag.take().is_some() {
                    return Some(
                        canvas::Action::publish(Interaction::Finish(
                            state.pending.take().map(Box::new),
                        ))
                        .and_capture(),
                    );
                }
            }
            canvas::Event::Window(iced::window::Event::Unfocused) => {
                state.modifiers = iced::keyboard::Modifiers::default();
                if state.drag.take().is_some() {
                    return Some(canvas::Action::publish(Interaction::Finish(
                        state.pending.take().map(Box::new),
                    )));
                }
                return state.pending.take().map(canvas::Action::publish);
            }
            canvas::Event::Window(iced::window::Event::RedrawRequested(_)) => {
                // Consume only the latest pointer location before this frame is drawn.
                return state.pending.take().map(canvas::Action::publish);
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { position: raw }) => {
                let hover_changed = state.hover != position;
                state.hover = position;
                if let Some(drag) = &mut state.drag {
                    // Use the OS event in window coordinates. Local widget coordinates can
                    // shift by a few float ULPs during layout and must not feed back into a drag.
                    let scale = f64::from(state.pixel_scale.get().unwrap_or(1.));
                    let px = (f64::from(raw.x) - f64::from(drag.last_pointer.x)) * scale;
                    let py = (f64::from(raw.y) - f64::from(drag.last_pointer.y)) * scale;
                    if px * px + py * py < DRAG_NOISE_RADIUS_PX * DRAG_NOISE_RADIUS_PX {
                        return hover_changed.then(canvas::Action::request_redraw);
                    }
                    drag.last_pointer = *raw;
                    // Capture the projection at mouse-down, then compute cursor
                    // displacement in f64; never invert a re-rendered marker position.
                    let projected = drag.mapping.projected(drag.bounds);
                    let dx = (f64::from(raw.x) - f64::from(drag.start.x))
                        / f64::from(drag.plot_size.width)
                        * projected.width();
                    let dy = (f64::from(raw.y) - f64::from(drag.start.y))
                        / f64::from(drag.plot_size.height)
                        * projected.height();
                    state.pending = Some(if let Some(axis) = drag.axis {
                        let horizontal = f64::from(raw.x) - f64::from(drag.start.x);
                        let vertical = f64::from(raw.y) - f64::from(drag.start.y);
                        Interaction::Bounds(drag.mapping.unprojected(scale_axis(
                            projected,
                            axis,
                            if axis == Axis::X {
                                horizontal
                            } else {
                                horizontal - vertical
                            },
                        )))
                    } else if let Some(target) = drag.target {
                        Interaction::Move(
                            target,
                            drag.mapping
                                .inverse(drag.mapping.forward(drag.origin) + C::new(dx, -dy)),
                        )
                    } else {
                        Interaction::Bounds(drag.mapping.unprojected(Bounds {
                            x_min: projected.x_min - dx,
                            x_max: projected.x_max - dx,
                            y_min: projected.y_min + dy,
                            y_max: projected.y_max + dy,
                        }))
                    });
                    return Some(canvas::Action::request_redraw().and_capture());
                }
                return hover_changed.then(canvas::Action::request_redraw);
            }
            canvas::Event::Mouse(mouse::Event::CursorLeft) => {
                state.hover = None;
                return Some(canvas::Action::request_redraw());
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                let p = position?;
                if *button == mouse::Button::Left
                    && let Some(axis) = axis_gutter(p, a)
                {
                    let start = cursor.position()?;
                    state.drag = Some(Drag {
                        axis: Some(axis),
                        target: None,
                        start,
                        last_pointer: start,
                        bounds: b.bounds,
                        mapping: b.mapping,
                        origin: C::new(0., 0.),
                        plot_size: a.size(),
                    });
                    return Some(canvas::Action::publish(Interaction::BeginGesture).and_capture());
                }
                if !a.contains(p) {
                    return None;
                }
                let hit = self.hit(p, b, a);
                if *button == mouse::Button::Right
                    && let Some((path, i, false)) = hit.and_then(input_target)
                {
                    return Some(
                        canvas::Action::publish(Interaction::RemovePathControl(path, i))
                            .and_capture(),
                    );
                }
                if *button == mouse::Button::Left || *button == mouse::Button::Middle {
                    if self.draw_path
                        && *button == mouse::Button::Left
                        && hit.is_none()
                        && self
                            .project
                            .settings
                            .path(self.active_path)
                            .is_some_and(|p| {
                                p.kind(self.project.settings.mode) != InputKind::Interval
                            })
                    {
                        return Some(
                            canvas::Action::publish(Interaction::AddControl(world(p, b, a)))
                                .and_capture(),
                        );
                    }
                    let target = if *button == mouse::Button::Middle {
                        None
                    } else {
                        hit
                    };
                    let origin = target
                        .and_then(|t| self.markers().into_iter().find(|m| m.0 == t))
                        .map_or(C::new(0., 0.), |m| m.1);
                    let start = cursor.position()?;
                    state.drag = Some(Drag {
                        axis: None,
                        target,
                        origin,
                        start,
                        last_pointer: start,
                        bounds: b.bounds,
                        mapping: b.mapping,
                        plot_size: a.size(),
                    });
                    return Some(canvas::Action::publish(Interaction::BeginGesture).and_capture());
                }
            }
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let p = position.filter(|p| a.contains(*p))?;
                let fast = state.modifiers.shift();
                // Some platforms translate Shift + vertical wheel into horizontal scroll.
                let axis = |x: f32, y: f32| if fast && y == 0. { x } else { y };
                let amount = match *delta {
                    mouse::ScrollDelta::Lines { x, y } => f64::from(axis(x, y)),
                    mouse::ScrollDelta::Pixels { x, y } => f64::from(axis(x, y)) / 40.,
                };
                let speed = if fast { 5. } else { 1. };
                let limit = if fast { 10. } else { 2.5 };
                let factor = (-amount * 0.12 * speed).exp().clamp(1. / limit, limit);
                let z = b.mapping.forward(world(p, b, a));
                let projected = b.mapping.projected(b.bounds);
                let next = Bounds {
                    x_min: z.re + (projected.x_min - z.re) * factor,
                    x_max: z.re + (projected.x_max - z.re) * factor,
                    y_min: z.im + (projected.y_min - z.im) * factor,
                    y_max: z.im + (projected.y_max - z.im) * factor,
                };
                return Some(
                    canvas::Action::publish(Interaction::Bounds(b.mapping.unprojected(next)))
                        .and_capture(),
                );
            }
            _ => {}
        }
        None
    }
    pub(super) fn pointer_interaction(
        &self,
        state: &State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if let Some(drag) = &state.drag {
            return match drag.axis {
                Some(Axis::X) => mouse::Interaction::ResizingHorizontally,
                Some(Axis::Y) => mouse::Interaction::ResizingVertically,
                None => mouse::Interaction::Grabbing,
            };
        }
        if let Some(axis) = cursor
            .position_in(bounds)
            .and_then(|p| axis_gutter(p, area(bounds.size())))
        {
            return match axis {
                Axis::X => mouse::Interaction::ResizingHorizontally,
                Axis::Y => mouse::Interaction::ResizingVertically,
            };
        }
        if let Some(p) = cursor
            .position_in(bounds)
            .filter(|p| area(bounds.size()).contains(*p))
        {
            if self
                .hit(p, self.project.settings.bounds, area(bounds.size()))
                .is_some()
            {
                mouse::Interaction::Grab
            } else if self.draw_path {
                mouse::Interaction::Crosshair
            } else {
                mouse::Interaction::default()
            }
        } else {
            mouse::Interaction::default()
        }
    }
}

pub(super) fn axis_gutter(p: Point, a: Rectangle) -> Option<Axis> {
    if p.x >= a.x && p.x <= a.x + a.width && p.y > a.y + a.height && p.y <= a.y + a.height + 40. {
        Some(Axis::X)
    } else if p.x >= 0. && p.x < a.x && p.y >= a.y && p.y <= a.y + a.height {
        Some(Axis::Y)
    } else {
        None
    }
}
pub(super) fn scale_axis(b: Bounds, axis: Axis, pixels: f64) -> Bounds {
    let factor = (-pixels / 180.).clamp(-30., 30.).exp();
    let mut next = b;
    match axis {
        Axis::X => {
            let mid = (b.x_min + b.x_max) * 0.5;
            let span = b.width() * factor;
            next.x_min = mid - span * 0.5;
            next.x_max = mid + span * 0.5;
        }
        Axis::Y => {
            let mid = (b.y_min + b.y_max) * 0.5;
            let span = b.height() * factor;
            next.y_min = mid - span * 0.5;
            next.y_max = mid + span * 0.5;
        }
    }
    if next.valid() { next } else { b }
}
pub(super) fn input_target(target: Target) -> Option<(usize, usize, bool)> {
    match target {
        Target::Control(i) => Some((0, i, false)),
        Target::Interval(i) => Some((0, i, true)),
        Target::PathControl(p, i) => Some((p, i, false)),
        Target::PathInterval(p, i) => Some((p, i, true)),
        _ => None,
    }
}
