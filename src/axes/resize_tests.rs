use super::*;
#[test]
fn asymmetric_view_scale_survives_collapse_resize_and_restore() {
    let old = iced::Size::new(600., 720.);
    let expanded = iced::Size::new(1100., 720.);
    for scale in [Scale::Linear, Scale::Log10, Scale::SymLog] {
        let mapping = Mapping {
            x: scale,
            y: scale,
            ..Default::default()
        };
        let original = mapping.fit(Bounds {
            x_min: -1.,
            x_max: 3.,
            y_min: -2.,
            y_max: 8.,
        });
        let before = mapping.projected(original);
        let next = resize_view(original, mapping, old, expanded);
        let after = mapping.projected(next);
        let aspect_before =
            before.width() / f64::from(old.width) / (before.height() / f64::from(old.height));
        let aspect_after = after.width()
            / f64::from(expanded.width)
            / (after.height() / f64::from(expanded.height));
        assert!((aspect_before - aspect_after).abs() < 1e-12);
        assert!((before.x_min + before.x_max - after.x_min - after.x_max).abs() < 1e-10);
        let restored = mapping.projected(resize_view(next, mapping, expanded, old));
        assert!((before.x_min - restored.x_min).abs() < 1e-10);
        assert!((before.x_max - restored.x_max).abs() < 1e-10);
        assert_eq!(before.y_min, restored.y_min);
        assert_eq!(before.y_max, restored.y_max);
    }
}
