//! Marker placement, front ordering and hit testing share the same geometry.
use super::*;

pub(super) struct Marker {
    pub(super) target: Target,
    pub(super) point: Point,
    pub(super) color: Color,
    pub(super) letter: String,
}
impl Plot<'_> {
    pub(super) fn markers(&self) -> Vec<(Target, C, Color)> {
        let s = &self.project.settings;
        let mut markers = Vec::new();
        {
            for node in self.points {
                let Some(entry) = self.project.entries.get(node.row) else {
                    continue;
                };
                if !entry.visible {
                    continue;
                }
                let c = color(parse_color(&entry.color).unwrap_or([0.8, 0.8, 0.8]));
                for (j, visible, z) in [
                    (0, s.nodes, node.z),
                    (1, s.values, node.value),
                    (2, s.weights, node.weight),
                ] {
                    if visible {
                        let z = if s.mode == Mode::Real {
                            C::new(z.re, entry.real_y[j])
                        } else {
                            z
                        };
                        markers.push((Target::Parameter(node.row, j), z, c));
                    }
                }
                if s.weighted_values {
                    let product = node.weight * node.value;
                    let point = if s.mode == Mode::Real {
                        C::new(product.re, (entry.real_y[1] + entry.real_y[2]) * 0.5)
                    } else {
                        product
                    };
                    markers.push((Target::Derived(node.row), point, c));
                }
            }
        }
        if s.input_path {
            for (index, path) in s.paths().enumerate().filter(|(_, p)| p.enabled) {
                let plan = self
                    .plan
                    .and_then(|p| p.paths.iter().find(|p| p.path == index));
                let kind = path.kind(s.mode);
                let c = color(s.input_line.color);
                if kind == InputKind::Interval {
                    if let Some(plan) = plan {
                        for (i, x) in plan.interval.into_iter().enumerate() {
                            markers.push((
                                if index == 0 {
                                    Target::Interval(i)
                                } else {
                                    Target::PathInterval(index, i)
                                },
                                C::new(x, 0.),
                                c,
                            ));
                        }
                    }
                } else {
                    let points =
                        plan.map_or(path.control_points.as_slice(), |p| p.points.as_slice());
                    for (i, p) in points
                        .iter()
                        .take(if kind == InputKind::Point { 1 } else { 64 })
                        .enumerate()
                    {
                        markers.push((
                            if index == 0 {
                                Target::Control(i)
                            } else {
                                Target::PathControl(index, i)
                            },
                            C::new(p[0], if s.mode == Mode::Real { 0. } else { p[1] }),
                            c,
                        ));
                    }
                }
            }
        }
        markers
    }
    pub(super) fn displayed_markers(&self, b: impl Into<Projection>, a: Rectangle) -> Vec<Marker> {
        let b = b.into();
        let mut markers: Vec<_> = self
            .markers()
            .into_iter()
            .filter_map(|(target, z, color)| {
                let point = screen(z, b, a);
                if !a.contains(point) {
                    return None;
                }
                let letter = match target {
                    Target::Parameter(_, j) => ["n", "v", "w"][j].to_owned(),
                    Target::Control(i)
                    | Target::Interval(i)
                    | Target::PathControl(_, i)
                    | Target::PathInterval(_, i) => (i + 1).to_string(),
                    Target::Derived(_) => "s".to_owned(),
                };
                Some(Marker {
                    target,
                    point,
                    color,
                    letter,
                })
            })
            .collect();
        markers.sort_by_key(|m| {
            if self.input_priority
                && input_target(m.target).is_some_and(|(p, _, _)| p == self.active_path)
            {
                3
            } else if self.input_priority && input_target(m.target).is_some() {
                2
            } else if self
                .emphasized_entry
                .is_some_and(|i| target_row(m.target) == Some(i))
            {
                1
            } else {
                0
            }
        });
        markers
    }
    pub(super) fn hit(&self, p: Point, b: impl Into<Projection>, a: Rectangle) -> Option<Target> {
        self.displayed_markers(b, a)
            .into_iter()
            .rev()
            .find(|m| {
                !matches!(m.target, Target::Derived(_))
                    && (self.editable_points || !matches!(m.target, Target::Parameter(..)))
                    && m.point.distance(p) < 9.
            })
            .map(|m| m.target)
    }
}
