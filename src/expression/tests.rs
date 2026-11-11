use super::*;
#[test]
fn arithmetic_and_complex() {
    for (s, expected) in [
        ("-2^2", C::new(-4., 0.)),
        ("2^3^2", C::new(512., 0.)),
        ("sin(pi/2)+3i", C::new(1., 3.)),
        ("sqrt(-1)", C::i()),
        ("(1+i)*(1-i)", C::new(2., 0.)),
        ("1e-3", C::new(0.001, 0.)),
    ] {
        assert!(
            (evaluate(s, &Variables::new()).expect(s) - expected).norm() < 1e-10,
            "{s}"
        );
    }
}
#[test]
fn invalid_and_deep_input() {
    for s in [
        "", "1/0", "missing", "sin(1", "2 +", "sin(1,2)", "p1[q]", "p1[n", "min()",
    ] {
        assert!(evaluate(s, &Variables::new()).is_err(), "{s}");
    }
    assert!(
        evaluate(
            &format!("{}1{}", "(".repeat(100), ")".repeat(100)),
            &Variables::new()
        )
        .is_err()
    );
}
