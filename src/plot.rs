use crate::{
    axes::{self, Mapping, Projection, StrokeStyle},
    model::{Bounds, Compiled, InputKind, Mode, Node, Project, parse_color},
    sampling::{Plan, RenderData},
};
use iced::advanced::Renderer as _;
use iced::{
    Color, Point, Rectangle, Renderer, Size, Theme, mouse,
    widget::canvas::{self, Frame, Geometry, Path, Stroke},
};
use num_complex::Complex64 as C;
use std::cell::Cell;
mod drawing;
mod interaction;
mod markers;
mod scene;
use drawing::{cursor_caption, draw_grid, draw_paths, draw_spectrum, guide_stroke, label};
use interaction::input_target;
use markers::Marker;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    Parameter(usize, usize),
    Control(usize),
    Interval(usize),
    PathControl(usize, usize),
    PathInterval(usize, usize),
    Derived(usize),
}
#[derive(Debug, Clone)]
pub enum Interaction {
    BeginGesture,
    Finish(Option<Box<Interaction>>),
    Bounds(Bounds),
    Move(Target, C),
    AddControl(C),
    RemoveControl(usize),
    RemovePathControl(usize, usize),
}
pub struct Plot<'a> {
    pub project: &'a Project,
    pub model: Option<&'a Compiled>,
    pub points: &'a [Node],
    pub plan: Option<&'a Plan>,
    pub data: Option<&'a RenderData>,
    pub revision: u64,
    pub draw_path: bool,
    pub spectrum: Option<&'a crate::numerics::Spectrum>,
    pub spectrum_filter: crate::numerics::filter::DisplayFilter,
    pub editable_points: bool,
    pub input_priority: bool,
    pub active_path: usize,
    pub emphasized_entry: Option<usize>,
    pub highlighted_entry: Option<usize>,
}
#[derive(Default)]
pub struct State {
    cache: canvas::Cache,
    axes_cache: canvas::Cache,
    grid_cache: canvas::Cache,
    circle_cache: canvas::Cache,
    circle_key: Cell<Option<AxesKey>>,
    axes_key: Cell<Option<AxesKey>>,
    pending: Option<Interaction>,
    key: Cell<u64>,
    drag: Option<Drag>,
    hover: Option<Point>,
    modifiers: iced::keyboard::Modifiers,
    pixel_scale: Cell<Option<f32>>,
}
#[derive(Clone, Copy, PartialEq)]
struct AxesKey {
    bounds: Bounds,
    grid_step: Option<f64>,
    grid_y_step: Option<f64>,
    guides: axes::Guides,
    mode: Mode,
    grid: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}
struct Drag {
    axis: Option<Axis>,
    target: Option<Target>,
    start: Point,
    bounds: Bounds,
    mapping: Mapping,
    origin: C,
    plot_size: Size,
    last_pointer: Point,
}
// Hysteresis around the last accepted window position, not a fixed pixel grid.
const DRAG_NOISE_RADIUS_PX: f64 = 0.125;
const CIRCLE_STEPS: usize = 2048;
static UNIT_CIRCLE: [[f64; 2]; CIRCLE_STEPS + 1] = unit_circle_points();
const fn unit_circle_points() -> [[f64; 2]; CIRCLE_STEPS + 1] {
    let theta = std::f64::consts::TAU / CIRCLE_STEPS as f64;
    let t2 = theta * theta;
    // Compile-time rotation, with Taylor truncation below f64 rounding at this angle.
    let cosine = 1. - t2 / 2. * (1. - t2 / 12. * (1. - t2 / 30. * (1. - t2 / 56.)));
    let sine = theta * (1. - t2 / 6. * (1. - t2 / 20. * (1. - t2 / 42. * (1. - t2 / 72.))));
    let mut points = [[0.; 2]; CIRCLE_STEPS + 1];
    let (mut x, mut y) = (1., 0.);
    let mut i = 0;
    while i < CIRCLE_STEPS {
        points[i] = [x, y];
        let next = x * cosine - y * sine;
        y = x * sine + y * cosine;
        x = next;
        i += 1;
    }
    points[CIRCLE_STEPS] = points[0];
    points
}
const BG: Color = Color::from_rgb(0.039, 0.055, 0.083);
const OUTPUT: Color = Color::from_rgb(0.42, 0.88, 0.94);
pub fn area(size: Size) -> Rectangle {
    Rectangle {
        x: 54.,
        y: 26.,
        width: (size.width - 74.).max(1.),
        height: (size.height - 66.).max(1.),
    }
}
fn screen(z: C, b: impl Into<Projection>, a: Rectangle) -> Point {
    let view = b.into();
    let z = view.mapping.forward(z);
    let b = view.mapping.projected(view.bounds);
    Point::new(
        (f64::from(a.x) + (z.re - b.x_min) / b.width() * f64::from(a.width)) as f32,
        (f64::from(a.y) + (b.y_max - z.im) / b.height() * f64::from(a.height)) as f32,
    )
}
fn world(p: Point, b: impl Into<Projection>, a: Rectangle) -> C {
    let view = b.into();
    let b = view.mapping.projected(view.bounds);
    view.mapping.inverse(C::new(
        b.x_min + (f64::from(p.x) - f64::from(a.x)) / f64::from(a.width) * b.width(),
        b.y_max - (f64::from(p.y) - f64::from(a.y)) / f64::from(a.height) * b.height(),
    ))
}
fn finite(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}
fn color(rgb: [f32; 3]) -> Color {
    Color::from_rgb(rgb[0], rgb[1], rgb[2])
}
fn alpha(mut c: Color, a: f32) -> Color {
    c.a = a;
    c
}
/// A separate canvas layer keeps raster domain colouring behind vector strokes.
pub struct Backdrop<'a> {
    pub project: &'a Project,
    pub data: Option<&'a RenderData>,
}
impl<Message> canvas::Program<Message> for Backdrop<'_> {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill_rectangle(
            Point::ORIGIN,
            bounds.size(),
            color(self.project.settings.guides.background),
        );
        let s = &self.project.settings;
        let a = area(bounds.size());
        if s.coloring_enabled()
            && let Some(data) = self.data
            && let Some(image) = &data.image
        {
            frame.with_clip(a, |frame| {
                let top = screen(
                    C::new(data.domain.x_min, data.domain.y_max),
                    Projection::new(s.bounds, s.guides.mapping),
                    a,
                );
                let bottom = screen(
                    C::new(data.domain.x_max, data.domain.y_min),
                    Projection::new(s.bounds, s.guides.mapping),
                    a,
                );
                if !finite(top) || !finite(bottom) {
                    return;
                }
                frame.draw_image(
                    Rectangle {
                        x: top.x,
                        y: top.y,
                        width: bottom.x - top.x,
                        height: bottom.y - top.y,
                    },
                    iced::advanced::image::Image::new(image.clone()).opacity(s.color_opacity),
                );
            });
        }
        vec![frame.into_geometry()]
    }
}
impl canvas::Program<Interaction> for Plot<'_> {
    type State = State;
    fn mouse_interaction(
        &self,
        state: &State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        self.pointer_interaction(state, bounds, cursor)
    }

    fn draw(
        &self,
        state: &State,
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        self.draw_scene(state, renderer, bounds)
    }

    fn update(
        &self,
        state: &mut State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Interaction>> {
        self.update_canvas(state, event, bounds, cursor)
    }
}
fn target_row(target: Target) -> Option<usize> {
    match target {
        Target::Parameter(i, _) | Target::Derived(i) => Some(i),
        _ => None,
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod frame_tests;

#[cfg(test)]
mod fractional_drag_tests;

#[cfg(test)]
mod unit_circle_tests;

#[cfg(test)]
mod nonlinear_drag_tests;

#[cfg(test)]
mod interval_handle_tests;
