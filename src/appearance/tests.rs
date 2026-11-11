use super::*;
#[test]
fn cyclic_palettes_close_and_colour_controls_preserve_special_values() {
    for palette in [Palette::Phase, Palette::Twilight, Palette::IceFire] {
        let a = palette.sample(0.);
        let b = palette.sample(1.);
        for j in 0..3 {
            assert!((a[j] - b[j]).abs() < 1e-6);
        }
    }
    let mut style = ColourStyle {
        contours: Contours::Off,
        shading: false,
        ..Default::default()
    };
    let z = Some(C::from_polar(1.3, 0.7));
    let base = Shader::new(style).color(z);
    style.palette = Palette::Viridis;
    assert_ne!(base, Shader::new(style).color(z));
    style.reverse = true;
    assert_ne!(
        Shader::new(style).color(z),
        Shader::new(ColourStyle {
            reverse: false,
            ..style
        })
        .color(z)
    );
    let z = Some(C::new(1., 0.));
    style.palette = Palette::Phase;
    let plain = Shader::new(style).color(z);
    for mode in [Contours::Phase, Contours::Modulus, Contours::Both] {
        style.contours = mode;
        assert_ne!(plain, Shader::new(style).color(z));
    }
    for palette in Palette::ALL {
        style.palette = palette;
        let shader = Shader::new(style);
        assert_eq!(shader.color(None), [242, 242, 250, 255]);
        assert_eq!(shader.color(Some(C::new(0., 0.))), [9, 11, 18, 255]);
        assert_eq!(shader.color(z)[3], 255);
    }
}
#[test]
fn arbitrary_pixel_widths_keep_the_memory_budget_and_hsv_round_trips() {
    for width in [1, 47, 777, 1024, 3073, 16384] {
        let (w, h) = image_size(width, 3.).expect("image");
        assert_eq!(w, width as usize);
        assert!(h > 0);
        assert!(w * h * 4 <= MAX_IMAGE_BYTES);
    }
    assert!(image_size(0, 1.).is_err());
    for rgb in [
        [0., 0., 0.],
        [1., 1., 1.],
        [0.2, 0.7, 0.6],
        [1., 0., 0.],
        [0., 1., 0.],
        [0., 0., 1.],
    ] {
        let [h, s, v] = rgb_to_hsv(rgb);
        let actual = hsv(h, s, v);
        for i in 0..3 {
            assert!((rgb[i] - actual[i]).abs() < 1e-6);
        }
    }
}
