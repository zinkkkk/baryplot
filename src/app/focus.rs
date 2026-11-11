//! Scoped keyboard focus in visual field order, with wrapping and scroll-to-reveal.
use super::*;
use iced::{
    Rectangle, Size, Vector,
    advanced::widget::{
        self, Id, Operation,
        operation::{self, Focusable, Outcome, Scrollable},
    },
};
impl App {
    pub(super) fn navigate_focus(&mut self, backwards: bool) -> Task<Message> {
        let scope = if self.appearance_ui.picker.is_some() {
            "picker-dialog"
        } else if self.appearance_ui.panel.is_some() {
            "settings-dialog"
        } else {
            self.panes.restore();
            "input-editor"
        };
        widget::operate(navigation(scope, backwards))
    }
}
pub(super) fn navigation<T: 'static>(scope: &'static str, backwards: bool) -> impl Operation<T> {
    operation::then(
        UnfocusOutside {
            scope: Id::new(scope),
            current: None,
        },
        move |_| scoped_focus(scope, backwards),
    )
}
fn scoped_focus<T: 'static>(scope: &'static str, backwards: bool) -> impl Operation<T> {
    let op = operation::then(operation::focusable::count(), move |count| {
        let target = if count.total == 0 {
            None
        } else {
            Some(match (count.focused, backwards) {
                (Some(i), true) => (i + count.total - 1) % count.total,
                (Some(i), false) => (i + 1) % count.total,
                (None, true) => count.total - 1,
                (None, false) => 0,
            })
        };
        FocusAt {
            target,
            current: 0,
            bounds: None,
        }
    });
    operation::scope(Id::new(scope), op)
}

struct UnfocusOutside {
    scope: Id,
    current: Option<Id>,
}
impl Operation<()> for UnfocusOutside {
    fn traverse(&mut self, f: &mut dyn FnMut(&mut dyn Operation)) {
        if self.current.as_ref() != Some(&self.scope) {
            f(self);
        }
        self.current = None;
    }
    fn container(&mut self, id: Option<&Id>, _: Rectangle, _: &Rectangle) {
        self.current = id.cloned();
    }
    fn focusable(&mut self, _: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
        state.unfocus();
    }
    fn finish(&self) -> Outcome<()> {
        Outcome::Some(())
    }
}

struct FocusAt {
    target: Option<usize>,
    current: usize,
    bounds: Option<Rectangle>,
}
impl<T: 'static> Operation<T> for FocusAt {
    fn traverse(&mut self, f: &mut dyn FnMut(&mut dyn Operation<T>)) {
        f(self);
    }
    fn focusable(&mut self, _: Option<&Id>, bounds: Rectangle, state: &mut dyn Focusable) {
        if self.target == Some(self.current) {
            state.focus();
            self.bounds = Some(bounds);
        } else {
            state.unfocus();
        }
        self.current += 1;
    }
    fn finish(&self) -> Outcome<T> {
        self.bounds.map_or(Outcome::None, |bounds| {
            Outcome::Chain(Box::new(Reveal { target: bounds }))
        })
    }
}
struct Reveal {
    target: Rectangle,
}
impl<T> Operation<T> for Reveal {
    fn traverse(&mut self, f: &mut dyn FnMut(&mut dyn Operation<T>)) {
        f(self);
    }
    fn scrollable(
        &mut self,
        _: Option<&Id>,
        bounds: Rectangle,
        content: Size,
        translation: Vector,
        state: &mut dyn Scrollable,
    ) {
        let top = bounds.y + translation.y;
        let bottom = top + bounds.height;
        let dy = if self.target.y < top + 6. {
            self.target.y - top - 6.
        } else if self.target.y + self.target.height > bottom - 6. {
            self.target.y + self.target.height - bottom + 6.
        } else {
            0.
        };
        if dy != 0. {
            state.scroll_to(
                operation::scrollable::AbsoluteOffset {
                    x: None,
                    y: Some(translation.y + dy),
                },
                operation::Animation::Instant,
                bounds,
                content,
            );
        }
    }
}
