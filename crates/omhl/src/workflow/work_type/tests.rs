use std::collections::BTreeMap;

use super::{effective_type, loosens};
use crate::workflow::config::{RequireAtom, WorkflowConfig};

fn table() -> BTreeMap<String, Vec<RequireAtom>> {
    WorkflowConfig::default().prefixes
}

#[test]
fn it_should_take_the_recorded_type_over_the_branch_prefix() {
    let resolved = effective_type(Some("fix"), &["feat"], &table());

    assert_eq!(resolved.as_deref(), Some("fix"));
}

#[test]
fn it_should_take_the_only_branch_prefix_when_no_type_is_recorded() {
    let resolved = effective_type(None, &["feat"], &table());

    assert_eq!(resolved.as_deref(), Some("feat"));
}

#[test]
fn it_should_not_resolve_without_a_recorded_type_and_without_a_branch() {
    assert_eq!(effective_type(None, &[], &table()), None);
}

#[test]
fn it_should_not_resolve_without_a_recorded_type_and_with_two_prefixes() {
    assert_eq!(effective_type(None, &["feat", "chore"], &table()), None);
}

#[test]
fn it_should_not_resolve_a_recorded_type_outside_the_effective_table() {
    assert_eq!(effective_type(Some("xyz"), &["feat"], &table()), None);
}

#[test]
fn it_should_not_resolve_a_branch_prefix_outside_the_effective_table() {
    assert_eq!(effective_type(None, &["release"], &table()), None);
}

fn atoms_of(name: &str) -> Vec<RequireAtom> {
    WorkflowConfig::default().prefixes[name].clone()
}

#[test]
fn it_should_refuse_a_change_to_a_type_that_drops_a_requirement() {
    assert!(loosens(&atoms_of("feat"), &atoms_of("chore")));
    assert!(loosens(&atoms_of("feat"), &atoms_of("refactor")));
}

#[test]
fn it_should_allow_a_change_that_keeps_or_adds_requirements() {
    assert!(!loosens(&atoms_of("chore"), &atoms_of("feat")));
    assert!(!loosens(&atoms_of("feat"), &atoms_of("fix")));
    assert!(!loosens(&atoms_of("bugfix"), &atoms_of("refactor")));
}

#[test]
fn it_should_refuse_a_removal_of_a_type_that_requires_something() {
    assert!(loosens(&atoms_of("feat"), &[]));
    assert!(!loosens(&atoms_of("chore"), &[]));
}

#[test]
fn it_should_refuse_refactor_to_bugfix_because_the_technical_premise_covers_the_agent_premise_only()
{
    // CA-005 lista refactor para bugfix como passando; pela regra do fecho ele afrouxa. Ver o relatório.
    assert!(loosens(&atoms_of("refactor"), &atoms_of("bugfix")));
}
