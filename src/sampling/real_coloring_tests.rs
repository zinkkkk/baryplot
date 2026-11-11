use super::*;
#[test]
fn real_mode_coloring_uses_zero_imaginary_coefficients_over_the_complex_plane() {
    let mut project = crate::model::reference_project(crate::model::Mode::Real);
    project.settings.real_coloring = true;
    project.settings.resolution = 128;
    for entry in &mut project.entries {
        entry.imaginary = Some([
            "missing_imaginary_variable".into(),
            "5".into(),
            "100".into(),
        ]);
    }
    let model = project
        .compile()
        .expect("real mode ignores stored imaginary fields");
    assert!(
        model
            .nodes
            .iter()
            .all(|n| n.z.im == 0. && n.value.im == 0. && n.weight.im == 0.)
    );
    let plan = plan(&project.settings, &model).expect("plan");
    let data = generate(
        &model,
        &project.settings,
        &plan,
        0,
        &Arc::new(AtomicU64::new(0)),
        |_| {},
    )
    .expect("full colour frame");
    assert!(
        data.samples
            .iter()
            .filter_map(|s| s.output)
            .all(|v| v.im == 0.)
    );
    let iced::widget::image::Handle::Rgba {
        width,
        height,
        pixels,
        ..
    } = data.image.expect("real-mode colour image")
    else {
        panic!("raw image")
    };
    assert_eq!(width, 128);
    let rgba: &[u8] = pixels.as_ref();
    let stride = width as usize * 4;
    assert_ne!(
        &rgba[..stride],
        &rgba[(height as usize / 2) * stride..(height as usize / 2 + 1) * stride],
        "colouring must evaluate the complex input plane, not repeat a real strip"
    );
}
