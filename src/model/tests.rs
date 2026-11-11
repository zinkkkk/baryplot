use super::*;
#[test]
fn interpolation_and_polynomial() {
    let p = crate::model::reference_project(crate::model::Mode::Real);
    let c = p.compile().expect("default");
    for n in &c.nodes {
        assert_eq!(c.evaluate(n.z), Some(n.value));
    }
    let v = c.evaluate(C::new(1., 0.)).expect("finite");
    assert!((v.re + 0.5).abs() < 1e-12);
}
#[test]
fn dependencies_locks_and_validation() {
    let mut p = Project {
        globals: "b = a*2\na = 1.2".into(),
        ..crate::model::reference_project(crate::model::Mode::Real)
    };
    p.entries[0].locks[0][0] = true;
    assert_eq!(p.compile().expect("globals").nodes[0].z.re, -1.);
    p.entries[1].expressions[0] = "-1".into();
    assert!(p.compile().is_err());
    p.entries[1].enabled = false;
    assert!(p.compile().is_ok());
}
#[test]
fn strict_real_and_zero_weight() {
    let mut p = crate::model::reference_project(crate::model::Mode::Real);
    p.entries[0].expressions[1] = "i".into();
    assert!(p.compile().is_err());
    p.settings.mode = Mode::Complex;
    assert!(p.compile().is_ok());
    p.entries[0].expressions[2] = "0".into();
    let c = p.compile().expect("zero weight");
    assert_ne!(c.evaluate(c.nodes[0].z), Some(C::i()));
}
