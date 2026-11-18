use crate::ports::ids::V7Ids;
use hexlog::IdGenerator;
use uuid::Variant;

#[test]
fn it_should_generate_uuid_v7_with_version_and_variant() {
    let id = V7Ids.next();

    assert_eq!(
        (id.get_version_num(), id.get_variant()),
        (7, Variant::RFC4122)
    );
}
