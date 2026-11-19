use super::*;

#[test]
fn it_should_declare_the_boolean_ok_as_derived() {
    let shape = PROFILE
        .success
        .iter()
        .find(|entry| entry.key == "ok")
        .map(|entry| entry.shape);

    assert_eq!(shape, Some(Shape::Derived));
}
