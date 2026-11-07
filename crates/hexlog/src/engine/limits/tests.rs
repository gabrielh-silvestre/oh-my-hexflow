use serde_json::{Value, json};

use super::{JSON_NESTING_MAX, json_issues, json_nesting_exceeds};

const NUMBER: &str = "number-not-safe-integer";
const DEPTH: &str = "max-depth-exceeded";
const BIG_STACK: usize = 64 * 1024 * 1024;
const TINY_STACK: usize = 128 * 1024;
const DEEP_LEVELS: usize = 20_000;

fn wrap_in_arrays(leaf: Value, containers: usize) -> Value {
    let mut value = leaf;
    for _ in 0..containers {
        value = Value::Array(vec![value]);
    }
    value
}

fn wrap_alternating(leaf: Value, containers: usize) -> Value {
    let mut value = leaf;
    for level in 0..containers {
        value = if level % 2 == 0 {
            json!({ "k": value })
        } else {
            Value::Array(vec![value])
        };
    }
    value
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

fn issue(pointer: &str, code: &'static str) -> (String, &'static str) {
    (pointer.to_owned(), code)
}

// O valor nasce e morre na thread grande (o Drop recursivo do Value precisa de pilha);
// só a travessia roda na thread pequena, que empresta o valor: recursão estoura ali.
fn exceeds_on_tiny_stack(levels: usize, maxes: [usize; 2]) -> [bool; 2] {
    std::thread::Builder::new()
        .stack_size(BIG_STACK)
        .spawn(move || {
            let deep = wrap_in_arrays(json!(1), levels);
            std::thread::scope(|scope| {
                std::thread::Builder::new()
                    .stack_size(TINY_STACK)
                    .spawn_scoped(scope, || maxes.map(|max| json_nesting_exceeds(&deep, max)))
                    .unwrap()
                    .join()
                    .unwrap()
            })
        })
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn it_should_count_nesting_in_containers_and_not_in_scalars() {
    let scalar_cases = [json!(7), json!("text"), json!(null), json!(true)];
    let shallow_cases = [
        (json!({}), 1),
        (json!([]), 1),
        (json!({"a": [1]}), 2),
        (json!([[[1]]]), 3),
    ];
    let deep_arrays = [
        (wrap_in_arrays(json!(1), JSON_NESTING_MAX), false),
        (wrap_in_arrays(json!(1), JSON_NESTING_MAX + 1), true),
    ];
    let deep_mixed = [
        (wrap_alternating(json!(1), JSON_NESTING_MAX), false),
        (wrap_alternating(json!(1), JSON_NESTING_MAX + 1), true),
    ];

    let thousands = exceeds_on_tiny_stack(DEEP_LEVELS, [DEEP_LEVELS, DEEP_LEVELS - 1]);

    assert_eq!(JSON_NESTING_MAX, 126);
    for scalar in &scalar_cases {
        assert!(!json_nesting_exceeds(scalar, 0), "scalar {scalar}");
    }
    for (value, depth) in &shallow_cases {
        assert!(!json_nesting_exceeds(value, *depth), "{value} at {depth}");
        assert!(
            json_nesting_exceeds(value, depth - 1),
            "{value} below {depth}"
        );
    }
    for (value, expected) in deep_arrays.iter().chain(&deep_mixed) {
        let exceeds = json_nesting_exceeds(value, JSON_NESTING_MAX);
        assert_eq!(exceeds, *expected);
    }
    assert_eq!(thousands, [false, true]);
}

#[test]
fn it_should_flag_unsafe_numbers_and_depth_in_document_order() {
    let unsafe_numbers = [
        "1.0",
        "1e2",
        "0.5",
        "-0",
        "-0.0",
        "9007199254740992",
        "-9007199254740992",
        "18446744073709551615",
    ];
    let safe_numbers = ["0", "9007199254740991", "-9007199254740991"];
    let scattered = parse(r#"{"b": [1.0, {"c": 0.5}], "a": 1e2, "d": 3, "e/f~g": 0.5}"#);
    let too_deep = json!({
        "a": wrap_in_arrays(json!(0.5), JSON_NESTING_MAX),
        "b": 0.5,
    });
    let at_limit = wrap_in_arrays(json!(0.5), JSON_NESTING_MAX);
    let deep_pointer = "/0".repeat(JSON_NESTING_MAX - 1);

    let from_root_numbers: Vec<_> = unsafe_numbers
        .iter()
        .map(|text| json_issues(&parse(text), 10))
        .collect();
    let from_safe_numbers: Vec<_> = safe_numbers
        .iter()
        .map(|text| json_issues(&json!([parse(text)]), 10))
        .collect();

    for (text, issues) in unsafe_numbers.iter().zip(&from_root_numbers) {
        assert_eq!(issues, &vec![issue("", NUMBER)], "number {text}");
    }
    for (text, issues) in safe_numbers.iter().zip(&from_safe_numbers) {
        assert!(issues.is_empty(), "number {text}");
    }
    assert_eq!(
        json_issues(&scattered, 10),
        vec![
            issue("/a", NUMBER),
            issue("/b/0", NUMBER),
            issue("/b/1/c", NUMBER),
            issue("/e~1f~0g", NUMBER),
        ]
    );
    assert_eq!(
        json_issues(&scattered, 2),
        vec![issue("/a", NUMBER), issue("/b/0", NUMBER)]
    );
    assert_eq!(
        json_issues(&too_deep, 10),
        vec![
            issue(&format!("/a{deep_pointer}"), DEPTH),
            issue("/b", NUMBER)
        ]
    );
    assert_eq!(
        json_issues(&at_limit, 10),
        vec![issue(&"/0".repeat(JSON_NESTING_MAX), NUMBER)]
    );
}
