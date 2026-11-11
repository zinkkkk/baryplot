use super::*;
#[test]
fn nonlinear_raster_pixels_evaluate_at_inverse_projected_centres() {
    let mut project = crate::model::reference_project(crate::model::Mode::Real);
    let s = &mut project.settings;
    s.real_coloring = true;
    s.resolution = 24;
    s.guides.mapping = crate::axes::Mapping {
        x: crate::axes::Scale::Log10,
        y: crate::axes::Scale::SymLog,
        threshold: 0.25,
    };
    s.bounds = Bounds {
        x_min: 0.01,
        x_max: 10.,
        y_min: -5.,
        y_max: 5.,
    };
    s.domain_margin = ["10".into(), "20".into()];
    let model = project.compile().unwrap();
    let s = &project.settings;
    let p = plan(s, &model).unwrap();
    assert!(p.domain.x_min > 0.);
    let projected = s.guides.mapping.projected(p.domain);
    let latest = Arc::new(AtomicU64::new(1));
    let data = generate(&model, s, &p, 1, &latest, |_| {}).unwrap();
    let Some(iced::widget::image::Handle::Rgba {
        width,
        height,
        pixels,
        ..
    }) = data.image
    else {
        panic!("pixels")
    };
    let shader = crate::appearance::Shader::new(s.colour_style);
    for (x, y) in [(0, 0), (7, 12), (width - 1, height - 1)] {
        let z = s.guides.mapping.inverse(C::new(
            projected.x_min + (x as f64 + 0.5) / width as f64 * projected.width(),
            projected.y_max - (y as f64 + 0.5) / height as f64 * projected.height(),
        ));
        let actual = &pixels[((y * width + x) * 4) as usize..((y * width + x) * 4 + 4) as usize];
        let expected = shader.color(model.evaluate(z));
        assert!(actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1));
    }
}
