use super::*;
#[test]
fn curve_endpoints_and_rational_arc() {
    let p = [[0., 0.], [1., 2.], [3., 1.], [4., 0.]];
    for kind in [Curve::Bezier, Curve::Line, Curve::Spline, Curve::Nurbs] {
        assert!((curve_point(&p, &[1.; 4], kind, 0.) - C::new(0., 0.)).norm() < 1e-10);
        assert!((curve_point(&p, &[1.; 4], kind, 1.) - C::new(4., 0.)).norm() < 1e-10);
    }
    let q = [[1., 0.], [1., 1.], [0., 1.]];
    let z = curve_point(&q, &[1., 0.5_f64.sqrt(), 1.], Curve::Nurbs, 0.5);
    assert!((z.norm() - 1.).abs() < 1e-12);
}
#[test]
fn cancellation_and_parallel_pixels() {
    let p = crate::model::reference_project(crate::model::Mode::Real);
    let model = p.compile().expect("default");
    let mut s = p.settings;
    s.mode = Mode::Complex;
    s.resolution = 128;
    let plan = plan(&s, &model).expect("plan");
    let latest = Arc::new(AtomicU64::new(1));
    assert!(generate(&model, &s, &plan, 0, &latest, |_| {}).is_none());
    let data = generate(&model, &s, &plan, 1, &latest, |_| {}).expect("render");
    assert!(data.image.is_some());
    assert_eq!(data.samples.len(), 1600);
}
