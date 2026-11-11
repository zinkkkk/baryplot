use super::*;
#[test]
fn transforms_round_trip_and_log_bounds_are_positive() {
    for scale in [Scale::Linear, Scale::Log10, Scale::SymLog] {
        for x in [-100., -1., -0.05, 0., 0.05, 1., 100.] {
            if scale == Scale::Log10 && x <= 0. {
                continue;
            }
            assert!((scale.inverse(scale.forward(x, 0.1), 0.1) - x).abs() < 1e-10);
        }
    }
    let m = Mapping {
        x: Scale::Log10,
        ..Default::default()
    };
    assert!(!m.valid(Bounds::default()));
    assert!(m.valid(m.fit(Bounds::default())));
    assert!(m.valid(Bounds {
        x_min: 1e-12,
        x_max: 1e-10,
        ..Bounds::default()
    }));
}
