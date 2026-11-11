use super::*;
#[test]
fn baked_circle_is_closed_with_cardinal_points_and_unit_radius() {
    assert_eq!(UNIT_CIRCLE.len(), 2049);
    assert_eq!(UNIT_CIRCLE[0], UNIT_CIRCLE[CIRCLE_STEPS]);
    for [x, y] in UNIT_CIRCLE {
        assert!((x.hypot(y) - 1.).abs() < 5e-13);
    }
    for (index, [x, y]) in [
        (0, [1., 0.]),
        (512, [0., 1.]),
        (1024, [-1., 0.]),
        (1536, [0., -1.]),
    ] {
        assert!((UNIT_CIRCLE[index][0] - x).abs() < 5e-13);
        assert!((UNIT_CIRCLE[index][1] - y).abs() < 5e-13);
    }
}
