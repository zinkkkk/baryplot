//! Multiline definition editing. Source is project data; editor cursor/scroll is UI state.
use super::view::{MUTED, WARNING, card};
use super::*;
use iced::{
    Element, Fill, Widget,
    widget::{button, column, container, pick_list, row, space, text, text_editor},
};

pub(super) struct NumericEditor {
    pub source: String,
    pub content: text_editor::Content,
}
impl App {
    pub(super) fn sync_numeric_editors(&mut self) {
        self.numeric_editors.retain(|&i, _| {
            self.project
                .globals
                .get(i)
                .is_some_and(|g| g.kind == model::DefinitionKind::NumericFunction)
        });
        for (i, g) in self
            .project
            .globals
            .iter()
            .enumerate()
            .filter(|(_, g)| g.kind == model::DefinitionKind::NumericFunction)
        {
            if self
                .numeric_editors
                .get(&i)
                .is_none_or(|editor| editor.source != g.expression)
            {
                self.numeric_editors.insert(
                    i,
                    NumericEditor {
                        source: g.expression.clone(),
                        content: text_editor::Content::with_text(&g.expression),
                    },
                );
            }
        }
    }
    pub(super) fn numeric_definition_view<'a>(
        &'a self,
        i: usize,
        g: &'a model::Definition,
    ) -> Element<'a, Message> {
        use model::DefinitionKind as Kind;
        let header = row![
            pick_list(
                Some(g.kind),
                [Kind::Variable, Kind::Function, Kind::NumericFunction],
                ToString::to_string
            )
            .on_select(move |k| Message::GlobalKind(i, k))
            .width(126)
            .text_size(11)
            .padding([6, 7]),
            self.input_field(InputKey::GlobalName(i), "name", &g.name, move |v| {
                Message::GlobalEdit(i, GlobalField::Name, v)
            }),
            container(self.input_field(
                InputKey::GlobalParameters(i),
                "x, y",
                &g.parameters,
                move |v| Message::GlobalEdit(i, GlobalField::Parameters, v)
            ))
            .width(90),
            button(text("×").size(16))
                .on_press(Message::RemoveGlobal(i))
                .style(button::text)
                .padding([4, 6]),
        ]
        .spacing(5)
        .align_y(iced::Center);
        let mut body = column![header].spacing(7);
        if let Some(editor) = self.numeric_editors.get(&i) {
            let invalid = self.issues.contains_key(&InputKey::GlobalExpression(i));
            body=body.push(text_editor(&editor.content)
                .id(format!("numeric-definition-{i}"))
                .on_action(move |action|Message::EditNumeric(i,action))
                .font(iced::Font::MONOSPACE).size(13)
                .height((editor.content.line_count().clamp(6,14) as f32)*19. + 16.)
                .padding(8)
                .key_binding(|press| {
                    use iced::keyboard::{Key,key::Named};
                    if matches!(press.key.as_ref(),Key::Named(Named::Tab))
                        || press.modifiers.command() && matches!(press.key.as_ref(),Key::Character(c) if c.eq_ignore_ascii_case("z") || c.eq_ignore_ascii_case("y")) {
                        None // Global history and field navigation own these shortcuts.
                    } else { text_editor::Binding::from_key_press(press) }
                })
                .style(move |theme,status| {
                    let mut style=text_editor::default(theme,status);
                    if invalid {style.border.color=WARNING;}
                    style
                }).boxed());
        }
        if let Some(error) = self.issues.get(&InputKey::GlobalExpression(i)) {
            body = body.push(text(format!("! {error}")).size(11).color(WARNING).boxed());
        }
        body=body.push(text("let total = 0; for k in 1..=20 { total += sin(k*x)/k; }\ntotal\nFinish with an expression, without ';'. Enter: newline · Tab: next field").size(10).color(MUTED).boxed());
        container(body).padding(8).width(Fill).style(card).boxed()
    }
}

impl App {
    pub(super) fn set_definition_kind(&mut self, i: usize, kind: model::DefinitionKind) {
        if let Some(g) = self.project.globals.get_mut(i) {
            g.kind = kind;
            if kind.is_function() && g.parameters.is_empty() {
                g.parameters = "x".into();
            }
        }
    }
    pub(super) fn edit_definition(&mut self, i: usize, field: GlobalField, value: String) {
        if let Some(g) = self.project.globals.get_mut(i) {
            match field {
                GlobalField::Name => g.name = value,
                GlobalField::Parameters => g.parameters = value,
                GlobalField::Expression => g.expression = value,
            }
        }
    }
    pub(super) fn add_definition(&mut self) {
        if self.project.globals.len() < 256 {
            let name = (1..)
                .map(|n| format!("g{n}"))
                .find(|n| {
                    self.project.globals.iter().all(|g| g.name != *n)
                        && self.project.entries.iter().all(|e| e.name != *n)
                })
                .expect("available name");
            self.project.globals.push(model::Definition {
                kind: model::DefinitionKind::Variable,
                name,
                parameters: String::new(),
                expression: "0".into(),
            });
        }
    }
    pub(super) fn remove_definition(&mut self, i: usize) {
        if i < self.project.globals.len() {
            self.project.globals.remove(i);
            self.numeric_editors = std::mem::take(&mut self.numeric_editors)
                .into_iter()
                .filter_map(|(index, editor)| {
                    (index != i).then_some((if index > i { index - 1 } else { index }, editor))
                })
                .collect();
        }
    }
    pub(super) fn edit_numeric_definition(
        &mut self,
        i: usize,
        action: iced::widget::text_editor::Action,
    ) -> bool {
        let action_is_edit = action.is_edit();
        self.sync_numeric_editors();
        if let Some(editor) = self.numeric_editors.get_mut(&i) {
            editor.content.perform(action);
            if action_is_edit {
                let source = editor.content.text();
                if source == editor.source {
                    return false;
                }
                editor.source = source;
                if let Some(g) = self.project.globals.get_mut(i) {
                    g.expression = editor.source.clone();
                }
            }
        }
        action_is_edit
    }
}

impl App {
    pub(super) fn definitions_view(&self) -> Element<'_, Message> {
        use model::DefinitionKind as Kind;
        let mut table = column![
            row![text("Global definitions").size(18),space().width(Fill),button(text("+ Add").size(12)).padding([6,10]).on_press(Message::AddGlobal)].align_y(iced::Center),
            text("Variables hold values; Function uses one expression; Numeric function adds a multiline body with locals, conditions and loops.").size(11).color(MUTED),
            row![text("Type").size(11).width(84),text("Name").size(11).width(86),text("Arguments").size(11).width(90),text("Expression").size(11).width(Fill),space().width(26)].spacing(5),
        ].spacing(10);
        for (i, g) in self.project.globals.iter().enumerate() {
            if g.kind == Kind::NumericFunction {
                table = table.push(self.numeric_definition_view(i, g));
                continue;
            }
            let args = if g.kind.is_function() {
                container(self.input_field(
                    InputKey::GlobalParameters(i),
                    "x, y",
                    &g.parameters,
                    move |s| Message::GlobalEdit(i, GlobalField::Parameters, s),
                ))
                .width(90)
                .boxed()
            } else {
                container(text("—").size(12).color(MUTED)).width(90).boxed()
            };
            let entry = row![
                pick_list(
                    Some(g.kind),
                    [Kind::Variable, Kind::Function, Kind::NumericFunction],
                    ToString::to_string
                )
                .on_select(move |k| Message::GlobalKind(i, k))
                .width(84)
                .text_size(11)
                .padding([6, 7]),
                container(
                    self.input_field(InputKey::GlobalName(i), "name", &g.name, move |s| {
                        Message::GlobalEdit(i, GlobalField::Name, s)
                    })
                )
                .width(86),
                args,
                container(self.input_field(
                    InputKey::GlobalExpression(i),
                    "expression",
                    &g.expression,
                    move |s| Message::GlobalEdit(i, GlobalField::Expression, s)
                ))
                .width(Fill),
                button(text("×").size(16))
                    .on_press(Message::RemoveGlobal(i))
                    .style(button::text)
                    .padding([4, 6])
                    .width(26),
            ]
            .spacing(5)
            .align_y(iced::Center);
            table = table.push(entry.boxed());
        }
        if self.project.globals.is_empty() {
            table = table.push(
                text("Add a variable or function to share it across all numeric inputs.")
                    .size(12)
                    .color(MUTED)
                    .boxed(),
            );
        }
        table.push(space().height(8).boxed()).push(text("Variable: a = 2\nFunction: wave(x, phase) = sin(x + phase)*a\nUse: wave(p1[n][re], pi/4)\nReferences can point forward to globals or active data rows. Missing names and cycles are marked in the affected cells.").size(12).color(MUTED).boxed())
            .push(self.project_controls()).boxed()
    }
}

#[cfg(test)]
mod tests;
