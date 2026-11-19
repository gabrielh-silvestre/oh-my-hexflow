use crate::workflow::omhl_leaf::Leaf;
use crate::workflow::role_allow::parse_role_allow;

fn file_with(omhl_allow: &str) -> String {
    format!("---\nname: x\n{omhl_allow}---\nbody\n")
}

#[test]
fn it_should_read_a_matrix_with_and_without_append() {
    let with = parse_role_allow(&file_with(
        "omhl-allow:\n  commands: [append, list]\n  append:\n    nodes: [decision]\n    relations: []\n    retire: []\n",
    ))
    .expect("valid matrix");
    let without =
        parse_role_allow(&file_with("omhl-allow:\n  commands: [list]\n")).expect("valid matrix");

    assert_eq!(with.commands, ["append", "list"]);
    assert!(with.append.is_some());
    assert!(without.append.is_none());
}

#[test]
fn it_should_refuse_what_has_the_wrong_shape() {
    let bad_matrices = [
        "omhl-allow:\n  commands: [list]\n  extra: 1\n",
        "omhl-allow:\n  commands: [append]\n",
        "omhl-allow:\n  commands: [list]\n  append:\n    nodes: []\n    relations: []\n    retire: []\n",
        "omhl-allow:\n  commands: [append]\n  append:\n    nodes: x\n    relations: []\n    retire: []\n",
        "omhl-allow:\n  commands: [append]\n  append:\n    nodes: [1]\n    relations: []\n    retire: []\n",
        "omhl-allow:\n  commands: [append]\n  append:\n    nodes: []\n    relations: []\n    retire: []\n    other: []\n",
        "omhl-allow:\n  commands: [list, 1]\n",
        "name: y\n",
        "omhl-allow:\n  commands: [list\n  append: {\n",
        "omhl-allow: list\n",
        "omhl-allow:\n  other: [list]\n",
        "omhl-allow:\n  commands: list\n",
    ];

    for matrix in bad_matrices {
        assert!(parse_role_allow(&file_with(matrix)).is_err(), "{matrix}");
    }
}

#[test]
fn it_should_refuse_an_append_list_that_is_not_a_list_of_strings() {
    for key in ["nodes", "relations", "retire"] {
        for value in ["premise", "[premise, 5]"] {
            let lists: String = ["nodes", "relations", "retire"]
                .iter()
                .map(|list| format!("    {list}: {}\n", if *list == key { value } else { "[]" }))
                .collect();
            let matrix = format!("omhl-allow:\n  commands: [append]\n  append:\n{lists}");

            assert!(
                parse_role_allow(&file_with(&matrix)).is_err(),
                "{key}: {value}"
            );
        }
    }
}

#[test]
fn it_should_refuse_a_file_whose_matrix_is_outside_the_frontmatter() {
    let in_body = "---\nname: x\n---\nomhl-allow:\n  commands: [list]\n";

    assert!(parse_role_allow(in_body).is_err());
    assert!(parse_role_allow("---\nname: x\n").is_err());
    assert!(parse_role_allow("omhl-allow:\n  commands: [list]\n").is_err());
}

#[test]
fn it_should_permit_read_only_leaves_to_every_role() {
    let allow = parse_role_allow(&file_with("omhl-allow:\n  commands: [list]\n")).expect("valid");

    assert!(allow.permits(&Leaf::ReadOnly));
    assert!(allow.permits(&Leaf::Named("list".to_owned())));
    assert!(!allow.permits(&Leaf::Named("attest".to_owned())));
}
