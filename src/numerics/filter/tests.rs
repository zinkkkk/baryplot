use super::*;
#[test]
fn location_and_pole_magnitude_filters_leave_zero_magnitudes_independent() {
    let spectrum = Spectrum {
        poles: vec![C::new(0.5, 0.), C::new(3., 4.), C::new(10., 0.)],
        zeros: vec![C::new(0., 0.), C::new(0., 2.)],
        ..Spectrum::default()
    };
    let filter = DisplayFilter {
        min_pole: Some(1.),
        max_pole: Some(6.),
        ..Default::default()
    };
    assert!(!filter.pole(&spectrum, 0));
    assert!(filter.pole(&spectrum, 1));
    assert!(!filter.pole(&spectrum, 2));
    assert!(filter.zero(&spectrum, 0) && filter.zero(&spectrum, 1));
    let real = DisplayFilter {
        locations: Locations::Real,
        ..filter
    };
    assert!(!real.pole(&spectrum, 1));
    assert!(real.zero(&spectrum, 0));
    assert!(!real.zero(&spectrum, 1));
    let complex = DisplayFilter {
        locations: Locations::Complex,
        ..filter
    };
    assert!(complex.pole(&spectrum, 1));
    assert!(!complex.zero(&spectrum, 0));
}
#[test]
fn both_current_chebfun_cleanup_criteria_hide_flagged_pairs() {
    let mut spectrum = Spectrum {
        poles: vec![C::new(0.2, 0.7), C::new(2., 0.)],
        zeros: vec![C::new(0.2 + 1e-14, 0.7), C::new(8., 0.)],
        residues: vec![Some(C::new(1e-16, 0.)), Some(C::new(1., 0.))],
        ..Spectrum::default()
    };
    let samples = [
        [C::new(-1., 0.), C::new(1., 0.)],
        [C::new(1., 0.), C::new(1., 0.)],
    ];
    spectrum.cleanup_metrics = Some(metrics(&spectrum.poles, &spectrum.zeros, &samples));
    for cleanup in [Cleanup::Method1, Cleanup::Method2] {
        let filter = DisplayFilter {
            cleanup,
            tolerance: 1e-13,
            ..Default::default()
        };
        assert!(!filter.pole(&spectrum, 0));
        assert!(filter.pole(&spectrum, 1));
        assert!(!filter.zero(&spectrum, 0));
        assert!(filter.zero(&spectrum, 1));
    }
    spectrum.poles[0] = C::new(0., 0.);
    spectrum.residues[0] = Some(C::new(1., 0.));
    spectrum.cleanup_metrics = Some(metrics(&spectrum.poles, &spectrum.zeros, &samples));
    assert!(
        DisplayFilter {
            cleanup: Cleanup::Method2,
            tolerance: 1e-13,
            ..Default::default()
        }
        .cleanup_pole(&spectrum, 0),
        "cleanup 2 filters real poles inside a real sample interval"
    );
}
