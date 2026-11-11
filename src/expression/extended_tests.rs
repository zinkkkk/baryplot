use super::*;
#[test]
fn remainder_operator_has_product_precedence_and_matches_rem() {
    for (source, expected) in [
        ("2+10%4*3", 8.),
        ("20/2%3", 1.),
        ("7%4%2", 1.),
        ("-5%3", -2.),
        ("5%-3", 2.),
        ("-5%-3", -2.),
        ("5.5%2", 1.5),
        ("-2^2%3", -1.),
        ("2^3%3", 2.),
        ("10%(1+2)", 1.),
        ("rem(-5,3)", -2.),
        ("mod(-5,3)", 1.),
        ("mod(5,-3)", 2.),
    ] {
        assert_eq!(
            evaluate(source, &Variables::new()).expect(source),
            C::new(expected, 0.),
            "{source}"
        );
    }
    for source in [
        "1%0",
        "rem(1,0)",
        "mod(1,0)",
        "(1+i)%2",
        "2%i",
        "rem(1+i,2)",
        "mod(1,2i)",
    ] {
        assert!(evaluate(source, &Variables::new()).is_err(), "{source}");
    }
}
#[test]
fn complex_identities_real_utilities_and_small_arguments() {
    for (source, expected) in [
        ("sinh(asinh(1+2i))", C::new(1., 2.)),
        ("cosh(acosh(1+2i))", C::new(1., 2.)),
        ("tanh(atanh(0.2+0.3i))", C::new(0.2, 0.3)),
        ("polar(2,pi/2)", C::new(0., 2.)),
        ("cis(pi/2)", C::i()),
        ("exp2(log2(1+2i))", C::new(1., 2.)),
        ("pow(-2,3)", C::new(-8., 0.)),
        ("cbrt(-8)", C::new(-2., 0.)),
        ("abs2(3+4i)", C::new(25., 0.)),
        ("sinc(0)", C::new(1., 0.)),
        ("lerp(1+i,3+5i,0.5)", C::new(2., 3.)),
        ("sign(3+4i)", C::new(0.6, 0.8)),
        ("mod(-5,3)", C::new(1., 0.)),
        ("rem(-5,3)", C::new(-2., 0.)),
        ("clamp(5,1,3)+hypot(3,4)", C::new(8., 0.)),
        ("degrees(atan2(1,0))", C::new(90., 0.)),
    ] {
        let value = evaluate(source, &Variables::new()).expect(source);
        assert!(
            (value - expected).norm() < 1e-11,
            "{source}: {value} != {expected}"
        );
    }
    for source in ["expm1(1e-15)", "log1p(1e-15)"] {
        assert!((evaluate(source, &Variables::new()).expect(source).re - 1e-15).abs() < 1e-28);
    }
    for source in [
        "clamp(1,3,2)",
        "hypot(1+i,2)",
        "polar(1,i)",
        "lerp(1,2)",
        "exp2(1,2)",
        "mod(1,0)",
    ] {
        assert!(evaluate(source, &Variables::new()).is_err(), "{source}");
    }
}
#[test]
fn user_functions_have_local_arguments_and_bounded_recursion_and_work() {
    let mut vars = Variables::new();
    vars.insert("x".into(), C::new(10., 0.));
    for (name, args, body) in [
        ("read", "", "x"),
        ("f", "x, y", "read()+x*y"),
        ("looping", "x", "looping(x)"),
        ("double", "x", "x+x"),
    ] {
        vars.define_function(
            name.into(),
            Function {
                parameters: parameters(args).expect("args"),
                body: Expression::parse(body).expect("body"),
            },
        );
    }
    assert_eq!(
        evaluate("f(2,3)", &vars).expect("local scope"),
        C::new(16., 0.)
    );
    assert_eq!(
        evaluate("f(i,2)", &vars).expect("complex argument"),
        C::new(10., 2.)
    );
    assert!(
        evaluate("f(2)", &vars)
            .expect_err("arity")
            .contains("2 arguments")
    );
    assert!(
        evaluate("looping(1)", &vars)
            .expect_err("recursion")
            .contains("recursion")
    );
    // An acyclic tree of calls can still grow exponentially; share one evaluation budget.
    vars.define_function(
        "branch".into(),
        Function {
            parameters: parameters("x").expect("args"),
            body: Expression::parse("f(x,x)+f(x,x)").expect("body"),
        },
    );
    for i in 0..16 {
        let body = if i == 0 {
            "x+x".into()
        } else {
            format!("b{}(x)+b{}(x)", i - 1, i - 1)
        };
        vars.define_function(
            format!("b{i}"),
            Function {
                parameters: parameters("x").expect("args"),
                body: Expression::parse(&body).expect("body"),
            },
        );
    }
    assert!(
        evaluate("b15(1)", &vars)
            .expect_err("operation budget")
            .contains("operation limit")
    );
}
