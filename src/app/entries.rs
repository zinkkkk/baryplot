//! Data-row insertion, removal, ordering and their transient selection state.
use super::*;

impl App {
    pub(super) fn clear_entry_selection(&mut self) {
        self.hovered_entry = None;
        self.last_hovered_entry = None;
        self.front_entry = None;
        self.entry_drag = None;
    }
    pub(super) fn add_entry(&mut self) {
        if self.project.entries.len() < 512 {
            let index = (0..)
                .find(|i| {
                    let name = format!("p{}", i + 1);
                    self.project.entries.iter().all(|e| e.name != name)
                        && self.project.globals.iter().all(|g| g.name != name)
                })
                .unwrap_or(0);
            let mut entry = Entry::new(index);
            if let Some(model) = &self.compiled {
                let node = (0..)
                    .find(|i| model.nodes.iter().all(|n| n.z != C::new(*i as f64, 0.)))
                    .unwrap_or(0);
                entry.expressions[0] = node.to_string();
            }
            if self.project.settings.mode == Mode::Complex {
                entry.split_components();
            }
            self.project.entries.push(entry);
        }
    }
    pub(super) fn clear_entries(&mut self) {
        self.project.entries.clear();
        self.project.entry_order.clear();
        self.data = None;
        self.spectrum.result = None;
        self.points.clear();
        self.clear_entry_selection();
    }
    pub(super) fn remove_entry(&mut self, i: usize) {
        if i < self.project.entries.len() {
            self.project.entry_order = self
                .project
                .ordered_entries()
                .into_iter()
                .filter(|&row| row != i)
                .map(|row| if row > i { row - 1 } else { row })
                .collect();
            self.project.entries.remove(i);
            let shift = |row: Option<usize>| {
                row.filter(|&row| row != i)
                    .map(|row| if row > i { row - 1 } else { row })
            };
            self.last_hovered_entry = shift(self.last_hovered_entry);
            self.hovered_entry = shift(self.hovered_entry);
            self.front_entry = shift(self.front_entry);
            self.entry_drag = None;
            self.points.retain(|p| p.row != i);
            for p in &mut self.points {
                if p.row > i {
                    p.row -= 1;
                }
            }
        }
    }
    pub(super) fn reorder_entry(&mut self, row: usize, slot: usize) {
        self.entry_drag = None;
        let mut order = self.project.ordered_entries();
        if let Some(from) = order.iter().position(|&i| i == row) {
            let slot = slot.min(order.len() - 1);
            if from != slot {
                order.remove(from);
                order.insert(slot, row);
                self.project.entry_order = order;
            }
            self.hovered_entry = None;
            self.last_hovered_entry = Some(row);
            self.visual_revision += 1;
        }
    }
}
