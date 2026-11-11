//! Workspace persistence and replacement; history remains in the application boundary.
use super::*;

impl App {
    pub(super) fn save_project(&mut self) {
        self.notice = match serde_json::to_string_pretty(&self.project)
            .map_err(|e| e.to_string())
            .and_then(|json| std::fs::write(&self.path, json).map_err(|e| e.to_string()))
        {
            Ok(()) => format!("Saved {}", self.path),
            Err(e) => format!("Save failed: {e}"),
        };
    }
    pub(super) fn load_project(&mut self) -> bool {
        match std::fs::read_to_string(&self.path)
            .map_err(|e| e.to_string())
            .and_then(|json| serde_json::from_str::<Project>(&json).map_err(|e| e.to_string()))
            .and_then(|p| {
                p.compile()
                    .and_then(|m| sampling::plan(&p.settings, &m).map(|_| p))
            }) {
            Ok(p) => {
                self.replace_project(p);
                self.notice = format!("Loaded {}", self.path);
            }
            Err(e) => {
                self.notice = format!("Load failed: {e}");
                return false;
            }
        }
        true
    }
    pub(super) fn use_preset(&mut self, mode: Mode) {
        self.replace_project(preset(mode));
        self.draw_path = false;
    }
    fn replace_project(&mut self, mut project: Project) {
        self.latest.fetch_add(1, Ordering::Relaxed);
        self.reset_numerical();
        project.settings.resolve_path_kinds();
        if project.settings.mode == Mode::Complex {
            project.split_components();
        }
        self.project = project;
        self.appearance_ui = Default::default();
        self.clear_entry_selection();
        self.axis = axis_strings(self.project.settings.bounds);
        self.active_path = 0;
        self.collapsed_paths.clear();
        self.sync_control_drafts();
        self.data = None;
        self.domain_key = None;
    }
}
