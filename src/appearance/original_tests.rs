use super::*;
#[test]
fn recovered_original_shader_matches_historical_pixel_values() {
    let shader = Shader::new(ColourStyle::classic());
    // Pixels from the pre-palette domain_color formula, including either side
    // of a power-of-two magnitude band. These detect hue/saturation/shading drift.
    for ((re, im), expected) in [
        ((1., 0.), [125, 35, 35, 255]),
        ((1.99, 0.), [201, 56, 56, 255]),
        ((2., 0.), [140, 39, 39, 255]),
        ((0.7, 0.4), [162, 103, 45, 255]),
        ((-2.3, 0.8), [49, 177, 136, 255]),
        ((-0.3, -1.7), [99, 53, 191, 255]),
        ((10., -12.), [222, 62, 196, 255]),
        ((1e-9, 1e-9), [63, 52, 17, 255]),
    ] {
        assert_eq!(shader.color(Some(C::new(re, im))), expected);
    }
    assert_eq!(shader.color(None), [242, 242, 250, 255]);
    assert_eq!(shader.color(Some(C::new(0., 0.))), [9, 11, 18, 255]);
}
