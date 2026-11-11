//! Selected-path editing; input paths remain independent from fitting and domain colouring.
use super::*;
use crate::model::{InputKind, InputPath};
impl App {
    pub(super) fn input(&self) -> &InputPath {
        self.project
            .settings
            .path(self.active_path)
            .unwrap_or(&self.project.settings.input)
    }
    pub(super) fn input_mut(&mut self) -> &mut InputPath {
        self.project
            .settings
            .path_mut(self.active_path)
            .expect("selected path")
    }
    pub(super) fn sync_control_drafts(&mut self) {
        self.control_drafts = self.input().drafts();
    }
    pub(super) fn select_path(&mut self, index: usize) {
        if self.project.settings.path(index).is_some() {
            self.active_path = index;
            self.sync_control_drafts();
            self.draw_path = false;
            let plotted =
                (self.fit.preview && self.fit.result.is_some()).then(|| self.points.clone());
            self.refresh_invalid_inputs();
            if let Some(points) = plotted {
                self.points = points;
            }
        }
    }
    pub(super) fn add_path(&mut self, kind: InputKind) {
        if self.project.settings.additional_paths.len() >= 31 {
            return;
        }
        let number = (1..)
            .find(|n| {
                self.project
                    .settings
                    .paths()
                    .all(|p| p.name != format!("Path {n}"))
            })
            .unwrap();
        let mut p = InputPath {
            name: format!("Path {number}"),
            input_kind: Some(kind),
            ..Default::default()
        };
        if kind == InputKind::Point {
            p.control_points = vec![[0., 0.]];
            p.curve_weights = vec!["1".into()];
        }
        self.project.settings.additional_paths.push(p);
        self.select_path(self.project.settings.additional_paths.len());
    }
    pub(super) fn remove_path(&mut self) {
        if self.project.settings.additional_paths.is_empty() {
            return;
        }
        let removed = self.active_path;
        self.collapsed_paths = self
            .collapsed_paths
            .iter()
            .filter_map(|&i| {
                if i == removed {
                    None
                } else {
                    Some(if i > removed { i - 1 } else { i })
                }
            })
            .collect();
        if self.active_path == 0 {
            self.project.settings.input = self.project.settings.additional_paths.remove(0);
        } else {
            self.project
                .settings
                .additional_paths
                .remove(self.active_path - 1);
        }
        self.active_path = self
            .active_path
            .min(self.project.settings.additional_paths.len());
        self.sync_control_drafts();
        self.draw_path = false;
    }
    pub(super) fn move_control(&mut self, path: usize, i: usize, z: C) {
        let mode = self.project.settings.mode;
        let Some(p) = self.project.settings.path_mut(path) else {
            return;
        };
        if i >= p.control_points.len() || !z.re.is_finite() || !z.im.is_finite() {
            return;
        }
        let mut drafts = p.drafts();
        if i >= drafts.len() {
            return;
        }
        p.control_points[i][0] = z.re;
        drafts[i][0] = model::number(C::new(z.re, 0.));
        if mode == Mode::Complex {
            p.control_points[i][1] = z.im;
            drafts[i][1] = model::number(C::new(z.im, 0.));
        }
        p.point_expressions = Some(drafts);
        if path == self.active_path {
            self.sync_control_drafts();
        }
    }
    pub(super) fn move_interval(&mut self, path: usize, i: usize, z: C) {
        if i >= 2 || !z.re.is_finite() || z.re.abs() >= 1e14 {
            return;
        }
        let Some(p) = self
            .plan
            .as_ref()
            .and_then(|p| p.paths.iter().find(|p| p.path == path))
        else {
            return;
        };
        if p.kind != InputKind::Interval {
            return;
        }
        let other = p.interval[1 - i];
        let x = if i == 0 {
            z.re.min(other.next_down())
        } else {
            z.re.max(other.next_up())
        };
        if let Some(p) = self.project.settings.path_mut(path) {
            p.interval[i] = model::number(C::new(x, 0.));
        }
    }
    pub(super) fn append_control(&mut self, z: Option<C>) {
        let kind = self.input().kind(self.project.settings.mode);
        let p = self.input_mut();
        let next = z
            .map(|z| [z.re, z.im])
            .unwrap_or_else(|| match p.control_points.as_slice() {
                [] => [0., 0.],
                [p] => [p[0] + 1., p[1]],
                v => {
                    let n = v.len();
                    [
                        2. * v[n - 1][0] - v[n - 2][0],
                        2. * v[n - 1][1] - v[n - 2][1],
                    ]
                }
            });
        let mut drafts = p.drafts();
        if kind == InputKind::Point {
            p.control_points.clear();
            p.curve_weights.clear();
            drafts.clear();
        }
        if p.control_points.len() < 64 {
            p.curve_weights.resize(p.control_points.len(), "1".into());
            p.control_points.push(next);
            p.curve_weights.push("1".into());
            drafts.push(next.map(|x| model::number(C::new(x, 0.))));
        }
        p.point_expressions = Some(drafts);
        self.sync_control_drafts();
    }
    pub(super) fn remove_control(&mut self, path: usize, i: usize) {
        let Some(p) = self.project.settings.path_mut(path) else {
            return;
        };
        if i >= p.control_points.len() {
            return;
        }
        let mut drafts = p.drafts();
        if i >= drafts.len() {
            return;
        }
        p.control_points.remove(i);
        if i < p.curve_weights.len() {
            p.curve_weights.remove(i);
        }
        drafts.remove(i);
        p.point_expressions = Some(drafts);
        if path == self.active_path {
            self.sync_control_drafts();
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod card_tests;
