//! Headless rendering checks. Common renderer/scene helpers live in the parent.
use super::*;

#[test]
fn grid_and_output_remain_visible_above_domain_image() {
    let mut renderer = software_renderer();
    for mode in [Mode::Real, Mode::Complex] {
        let mut app = completed_app(mode);
        let visible = snapshot(&app, &mut renderer);
        let cyan = |pixels: &tiny_skia::Pixmap| {
            pixels
                .data()
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[0] < 150 && p[1] > 200 && p[2] > 215)
                .count()
        };
        let output_pixels = cyan(&visible);
        assert!(
            output_pixels > 500,
            "{mode}: output must be visible, got {output_pixels} pixels"
        );
        let _ = app.update(Message::Flag(Flag::Output, false));
        let hidden = snapshot(&app, &mut renderer);
        assert!(
            cyan(&hidden) < output_pixels / 4,
            "output toggle must hide the curve"
        );
        let _ = app.update(Message::Flag(Flag::Grid, false));
        let no_grid = snapshot(&app, &mut renderer);
        let changed = hidden
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .zip(no_grid.data().as_chunks::<4>().0)
            .enumerate()
            .filter(|(i, (a, b))| {
                i % 1520 > 650
                    && (60..900).contains(&(i / 1520))
                    && (0..3).map(|j| a[j].abs_diff(b[j]) as u32).sum::<u32>() > 20
            })
            .count();
        assert!(
            changed > 2000,
            "{mode}: grid toggle must affect the plot, got {changed} pixels"
        );
    }
}

#[test]
#[ignore = "CPU scene-preparation timing; run with --release --ignored --nocapture"]
fn benchmark_drag_scene() {
    use std::time::Instant;
    fn report(name: &str, mut times: Vec<f64>) {
        times.sort_by(f64::total_cmp);
        println!(
            "{name}: median {:.3} ms, p95 {:.3} ms",
            times[times.len() / 2],
            times[times.len() * 95 / 100]
        );
    }
    for mode in [Mode::Real, Mode::Complex] {
        let mut app = completed_app(mode);
        app.busy = true; // Measure frame preparation independently from asynchronous texture work.
        let mut renderer = software_renderer();
        let mut cache = iced_runtime::user_interface::Cache::default();
        let theme = crate::theme();
        let mut update_times = Vec::new();
        let mut layout_times = Vec::new();
        let mut draw_times = Vec::new();
        for i in 0..256 {
            let start = Instant::now();
            let _ = app.update(Message::Plot(crate::plot::Interaction::Move(
                crate::plot::Target::Parameter(0, 1),
                num_complex::Complex64::new(0.5 + (i as f64 * 0.04).sin(), 0.5),
            )));
            let updated = Instant::now();
            let mut ui =
                UserInterface::build(app.view(), Size::new(1520., 960.), cache, &mut renderer);
            let laid_out = Instant::now();
            ui.draw(
                &mut renderer,
                &theme,
                &renderer::Style {
                    text_color: iced::Color::WHITE,
                },
                mouse::Cursor::Unavailable,
            );
            let drawn = Instant::now();
            cache = ui.into_cache();
            if i >= 32 {
                update_times.push((updated - start).as_secs_f64() * 1000.);
                layout_times.push((laid_out - updated).as_secs_f64() * 1000.);
                draw_times.push((drawn - laid_out).as_secs_f64() * 1000.);
            }
        }
        report(&format!("{mode} update + sample"), update_times);
        report(&format!("{mode} UI rebuild + layout"), layout_times);
        report(
            &format!("{mode} CPU scene recording (software backend)"),
            draw_times,
        );
    }
}

#[test]
fn grid_toggle_also_hides_the_zero_axis_cross() {
    let mut app = completed_app(Mode::Real);
    let s = &mut app.project.settings;
    s.grid = false;
    s.output = false;
    s.input_path = false;
    s.nodes = false;
    s.values = false;
    s.weights = false;
    s.weighted_values = false;
    app.visual_revision += 1;
    let image = snapshot(&app, &mut software_renderer());
    // This region lies strictly inside the graph, away from its border and tick labels.
    for y in 120..850 {
        for x in 700..1450 {
            let p = &image.data()[(y * 1520 + x) * 4..][..3];
            assert!(
                p.iter().all(|v| *v < 30),
                "unexpected axis/grid stroke at ({x},{y}): {p:?}"
            );
        }
    }
}

#[test]
fn transient_invalid_input_keeps_domain_colours_visible() {
    let mut app = completed_app(Mode::Complex);
    let _ = app.update(Message::Edit(0, 1, "sin(".into()));
    assert!(app.error.is_some());
    let image = snapshot(&app, &mut software_renderer());
    let mut coloured = 0;
    for y in 120..850 {
        for x in 700..1450 {
            let p = &image.data()[(y * 1520 + x) * 4..][..3];
            if p.iter()
                .max()
                .expect("rgb")
                .saturating_sub(*p.iter().min().expect("rgb"))
                > 35
            {
                coloured += 1;
            }
        }
    }
    assert!(
        coloured > 100_000,
        "an invalid intermediate drag must hold the previous colour image"
    );
}

#[test]
fn real_view_shows_complex_roots_by_default_and_can_filter_them() {
    use crate::numerics::{
        Spectrum,
        filter::{DisplayFilter, Locations},
    };
    let mut app = completed_app(Mode::Real);
    app.project.settings.analysis.poles = true;
    app.project.settings.analysis.zeros = true;
    app.spectrum.result = Some(Arc::new(Spectrum {
        poles: vec![C::new(0.7, 0.6)],
        zeros: vec![C::new(-0.7, 0.8)],
        ..Spectrum::default()
    }));
    let red = |pixmap: &tiny_skia::Pixmap| {
        pixmap
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] > 220 && p[1] < 150 && p[2] < 160)
            .count()
    };
    let blue = |pixmap: &tiny_skia::Pixmap| {
        pixmap
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] < 150 && p[1] > 150 && p[1] < 200 && p[2] > 245)
            .count()
    };
    let mut renderer = software_renderer();
    let visible = snapshot(&app, &mut renderer);
    assert!(
        blue(&visible) > 25,
        "complex zero circle and label must be visible in real mode"
    );
    assert!(
        // The compact pane puts the small cross on different subpixels. Keep a
        // margin for antialiasing while requiring a visible cross and label.
        red(&visible) > 20,
        "complex pole cross and label must be visible in real mode, got {} pixels",
        red(&visible)
    );
    app.spectrum.filter = DisplayFilter {
        locations: Locations::Real,
        ..Default::default()
    };
    let hidden = snapshot(&app, &mut renderer);
    assert_eq!(blue(&hidden), 0, "real-only filter must hide complex zeros");
    assert_eq!(red(&hidden), 0, "real-only filter must hide complex poles");
    app.spectrum.filter = DisplayFilter::default();
    app.spectrum.result = Some(Arc::new(Spectrum {
        poles: vec![C::new(0., 10.)],
        ..Spectrum::default()
    }));
    let offscreen = snapshot(&app, &mut renderer);
    assert_eq!(
        red(&offscreen),
        0,
        "offscreen complex poles must not pile labels onto the viewport edge"
    );
    app.spectrum.result = Some(Arc::new(Spectrum {
        poles: vec![C::new(0.7, 0.)],
        ..Spectrum::default()
    }));
    let real_pole = snapshot(&app, &mut renderer);
    let changed = real_pole
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(offscreen.data().as_chunks::<4>().0)
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        changed > 30 && changed < 500,
        "real poles must draw a small cross and label, not a vertical guide: {changed} changed pixels"
    );
}
