extern crate mech_core;

use mech_core::{Quantity, ToQuantity, QuantityMath, make_quantity};

#[test]
fn quantities_base() {
    let x = make_quantity(1, 3, 1);
    let y = make_quantity(1, -3, 1);
    let added = x.add(y);
    assert!(x.is_number());
    assert!(!x.is_other());
    assert_eq!(x.mantissa(), 1);
    assert_eq!(y.mantissa(), 1);
    assert_eq!(x.range(), 3);
    assert_eq!(y.range(), -3);
    assert_eq!(added.mantissa(), 1000001);
    assert_eq!(added.range(), -3);
    let added_reverse = y.add(x);
    assert_eq!(added_reverse.mantissa(), 1000001);
    assert_eq!(added_reverse.range(), -3);
}

#[test]
fn quantities_base_sub() {
    let x = make_quantity(1, 3, 1);
    let y = make_quantity(1, -3, 1);
    let sub = x.sub(y);
    assert_eq!(sub.mantissa(), 999999);
    assert_eq!(sub.range(), -3);
}

#[test]
fn quantities_base_multiply() {
    let x = make_quantity(1, 3, 1);
    let y = make_quantity(1, -3, 1);
    let sub = x.multiply(y);
    assert_eq!(sub.mantissa(), 1);
    assert_eq!(sub.range(), 0);
}

#[test]
fn quantities_base_divide() {
    let x = 1.to_quantity();
    let y = 2.to_quantity();
    assert_eq!(x.divide(y).to_float(), 0.5);
}

#[test]
fn quantities_base_add_float() {
    let x = 0.1.to_quantity();
    let y = 0.2.to_quantity();
    assert_eq!(x.add(y).to_float(), 0.3);
}

#[test]
fn quantities_base_add_different_range_float() {
    let x = 0.2.to_quantity();
    let y = 0.3.to_quantity();
    assert_eq!(x.add(y).to_float(), 0.5);
}

#[test]
fn quantities_base_add_01_02_03() {
    let x = 0.1.to_quantity();
    let y = 0.2.to_quantity();
    let z = 0.3.to_quantity();
    assert_eq!(x.add(y.add(z)).to_float(), 0.6);
}

#[test]
fn quantities_base_associativity() {
    let x = 0.1.to_quantity();
    let y = 0.2.to_quantity();
    let z = 0.3.to_quantity();
    assert_eq!(z.add(x.add(y)).to_float(), 0.6);
}

#[test]
fn quantities_base_add_subtract() {
    let x = 0.1.to_quantity();
    let y = 0.2.to_quantity();
    let z = 0.3.to_quantity();
    assert_eq!((z.add(x.add(y))).sub(z).sub(y).to_float(), 0.1);
}

#[test]
fn quantities_base_float() {
    let x = 1.2;
    let y = 1.1;
    let z = 0.5;
    assert_eq!(x.to_quantity().to_float(), x);
    assert_eq!(y.to_quantity().to_float(), y);
    assert_eq!(z.to_quantity().to_float(), z);
    println!("{}", x.to_quantity().to_string());
    println!("{}", y.to_quantity().to_string());
    println!("{}", z.to_quantity().to_string());
}