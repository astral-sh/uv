use super::{NodeId, INTERNER};
use crate::MarkerExpression;

fn expr(s: &str) -> NodeId {
    INTERNER
        .lock()
        .expression(MarkerExpression::from_str(s).unwrap().unwrap())
}

#[test]
fn basic() {
    let m = || INTERNER.lock();
    let extra_foo = expr("extra == 'foo'");
    assert!(!extra_foo.is_false());

    let os_foo = expr("os_name == 'foo'");
    let extra_and_os_foo = m().or(extra_foo, os_foo);
    assert!(!extra_and_os_foo.is_false());
    assert!(!m().and(extra_foo, os_foo).is_false());

    let trivially_true = m().or(extra_and_os_foo, extra_and_os_foo.not());
    assert!(!trivially_true.is_false());
    assert!(trivially_true.is_true());

    let trivially_false = m().and(extra_foo, extra_foo.not());
    assert!(trivially_false.is_false());

    let e = m().or(trivially_false, os_foo);
    assert!(!e.is_false());

    let extra_not_foo = expr("extra != 'foo'");
    assert!(m().and(extra_foo, extra_not_foo).is_false());
    assert!(m().or(extra_foo, extra_not_foo).is_true());

    let os_geq_bar = expr("os_name >= 'bar'");
    assert!(!os_geq_bar.is_false());

    let os_le_bar = expr("os_name < 'bar'");
    assert!(m().and(os_geq_bar, os_le_bar).is_false());
    assert!(m().or(os_geq_bar, os_le_bar).is_true());

    let os_leq_bar = expr("os_name <= 'bar'");
    assert!(!m().and(os_geq_bar, os_leq_bar).is_false());
    assert!(m().or(os_geq_bar, os_leq_bar).is_true());
}

#[test]
fn version() {
    let m = || INTERNER.lock();
    let eq_3 = expr("python_version == '3'");
    let neq_3 = expr("python_version != '3'");
    let geq_3 = expr("python_version >= '3'");
    let leq_3 = expr("python_version <= '3'");

    let eq_2 = expr("python_version == '2'");
    let eq_1 = expr("python_version == '1'");
    assert!(m().and(eq_2, eq_1).is_false());

    assert_eq!(eq_3.not(), neq_3);
    assert_eq!(eq_3, neq_3.not());

    assert!(m().and(eq_3, neq_3).is_false());
    assert!(m().or(eq_3, neq_3).is_true());

    assert_eq!(m().and(eq_3, geq_3), eq_3);
    assert_eq!(m().and(eq_3, leq_3), eq_3);

    assert_eq!(m().and(geq_3, leq_3), eq_3);

    assert!(!m().and(geq_3, leq_3).is_false());
    assert!(m().or(geq_3, leq_3).is_true());
}

#[test]
fn simplify() {
    let m = || INTERNER.lock();
    let x86 = expr("platform_machine == 'x86_64'");
    let not_x86 = expr("platform_machine != 'x86_64'");
    let windows = expr("platform_machine == 'Windows'");

    let a = m().and(x86, windows);
    let b = m().and(not_x86, windows);
    assert_eq!(m().or(a, b), windows);
}
