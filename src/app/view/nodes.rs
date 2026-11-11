//! Manual data-row cards and drag previews.
use super::*;
use iced::widget::column;

impl App {
    pub(super) fn nodes_header(&self) -> Element<'_, Message> {
        column![
            row![
                text(format!(
                    "Data · {} entries · {} active",
                    self.project.entries.len(),
                    self.project.entries.iter().filter(|e| e.enabled).count()
                ))
                .size(14),
                space().width(Fill),
                button(text("Clear all entries").size(12))
                    .style(button::danger)
                    .padding([6, 10])
                    .on_press(Message::ClearEntries)
            ]
            .align_y(iced::Center),
            row![
                button(text("+ Add row").size(12))
                    .on_press(Message::Add)
                    .padding([6, 12]),
                space().width(Fill)
            ]
        ]
        .spacing(12)
        .boxed()
    }
    pub(super) fn nodes_view(&self) -> Element<'_, Message> {
        if self.fit.preview && self.fit.result.is_some() {
            return self.fitted_nodes_view();
        }
        let mut rows = Vec::new();
        for i in self.project.ordered_entries() {
            let entry = &self.project.entries[i];
            let c = model::parse_color(&entry.color)
                .map_or(MUTED, |v| Color::from_rgb(v[0], v[1], v[2]));
            let rgb = model::parse_color(&entry.color).unwrap_or([0.8, 0.8, 0.8]);
            let top = row![
                container(text("≡").size(16))
                    .width(28)
                    .height(26)
                    .center_x(28)
                    .center_y(26),
                button(crate::picker::swatch(rgb, 20., 19.))
                    .padding(2)
                    .style(button::text)
                    .on_press(Message::Appearance(appearance::Action::Pick(
                        crate::picker::Target::Entry(i)
                    ))),
                container(
                    self.input_field(InputKey::Name(i), "name", &entry.name, move |v| {
                        Message::Name(i, v)
                    })
                )
                .width(70),
                checkbox(entry.enabled)
                    .label("On")
                    .size(12)
                    .text_size(10)
                    .on_toggle(move |v| Message::Enabled(i, v)),
                checkbox(entry.visible)
                    .label("Show")
                    .size(12)
                    .text_size(10)
                    .on_toggle(move |v| Message::Visible(i, v)),
                space().width(Fill),
                widget::tooltip(
                    button(text("↑").size(14))
                        .padding([2, 5])
                        .style(if self.front_entry == Some(i) {
                            button::primary
                        } else {
                            button::text
                        })
                        .on_press(Message::FrontEntry(i)),
                    text("Pin points to front").size(11),
                    widget::tooltip::Position::Bottom
                ),
                button(text("×").size(14))
                    .padding([2, 5])
                    .style(button::text)
                    .on_press(Message::Remove(i))
            ]
            .spacing(5)
            .align_y(iced::Center);
            let mut inputs = row![].spacing(7);
            for (j, label) in ["Node · n", "Value · v", "Weight · w"].iter().enumerate() {
                let mut locks = row![
                    checkbox(entry.locks[j][0])
                        .label(if self.project.settings.mode == Mode::Real {
                            "Integer"
                        } else {
                            "Re #"
                        })
                        .size(12)
                        .text_size(10)
                        .on_toggle(move |v| Message::Lock(i, j, 0, v))
                ]
                .spacing(7);
                if self.project.settings.mode == Mode::Complex {
                    locks = locks.push(
                        checkbox(entry.locks[j][1])
                            .label("Im #")
                            .size(12)
                            .text_size(10)
                            .on_toggle(move |v| Message::Lock(i, j, 1, v))
                            .boxed(),
                    );
                }
                let evaluated = self
                    .points
                    .iter()
                    .find(|n| n.row == i)
                    .map(|n| model::number([n.z, n.value, n.weight][j]))
                    .unwrap_or_else(|| "—".into());
                let real_input = self.input_field(
                    InputKey::Parameter(i, j, 0),
                    "expression",
                    &entry.expressions[j],
                    move |v| Message::Edit(i, j, v),
                );
                let mut parameter = column![text(*label).size(11).color(MUTED)]
                    .spacing(3)
                    .width(Fill);
                if self.project.settings.mode == Mode::Complex {
                    let im = entry
                        .imaginary
                        .as_ref()
                        .map(|v| v[j].as_str())
                        .unwrap_or("0");
                    parameter = parameter
                        .push(
                            row![text("Re").size(10).color(MUTED).width(18), real_input]
                                .align_y(iced::Center)
                                .spacing(4)
                                .boxed(),
                        )
                        .push(
                            row![
                                text("Im").size(10).color(MUTED).width(18),
                                self.input_field(
                                    InputKey::Parameter(i, j, 1),
                                    "expression",
                                    im,
                                    move |v| Message::EditImaginary(i, j, v)
                                )
                            ]
                            .align_y(iced::Center)
                            .spacing(4)
                            .boxed(),
                        );
                } else {
                    parameter = parameter.push(real_input);
                }
                inputs = inputs.push(
                    parameter
                        .push(locks.boxed())
                        .push(text(format!("= {evaluated}")).size(10).color(c).boxed())
                        .boxed(),
                );
            }
            let row_invalid=self.issues.keys().any(|key|matches!(*key,InputKey::Parameter(row,_,_)|InputKey::Name(row)|InputKey::Color(row) if row==i));
            let emphasized = self.hovered_entry == Some(i);
            let content = widget::mouse_area(
                container(column![top, inputs].spacing(5))
                    .id(format!("data-row-{i}"))
                    .padding(8)
                    .style(move |theme| {
                        let mut style = card(theme);
                        if row_invalid {
                            style.border.color = WARNING;
                        } else if emphasized {
                            style.border.color = c;
                        }
                        style
                    }),
            )
            .on_enter(Message::HoverEntry(i, true))
            .on_exit(Message::HoverEntry(i, false))
            .boxed();
            let values = self
                .points
                .iter()
                .find(|n| n.row == i)
                .map(|n| [n.z, n.value, n.weight].map(model::number))
                .unwrap_or_else(|| entry.expressions.clone());
            rows.push(crate::reorder::Item {
                id: i,
                content,
                name: entry.name.clone(),
                values,
                color: c,
            });
        }
        let rows =
            column![crate::reorder::Rows::new(rows, self.entry_drag.map(|drag| drag.from)).boxed()]
                .spacing(12);
        rows
            .push(self.project_controls())
            .push(button(text("Global variables and functions →").size(12)).on_press(Message::Tab(Tab::Definitions)).style(button::text).boxed())
            .push(text("r(z) = Σ wⱼ fⱼ / (z − zⱼ)  /  Σ wⱼ / (z − zⱼ)").size(12).color(MUTED).boxed())
            .push(text("n · node   v · value   w · weight   s · weight × value").size(11).color(MUTED).boxed())
            .push(text(if self.project.settings.mode==Mode::Real {
                "Drag freely: x = parameter value, y = layout only. Integer locks snap x."
            }else{
                "Drag freely: x = Re, y = Im. Locks snap each selected component."
            }).size(11).color(MUTED).boxed())
            .push(text("References: p1[n], p1[v], p1[w], p1[n][im]. Data rows require an explicit field. Forward references are supported.").size(11).color(MUTED).boxed())
            .push(space().height(14).boxed()).boxed()
    }
}
