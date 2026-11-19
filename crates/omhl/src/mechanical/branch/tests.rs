use crate::mechanical::branch::{
    BranchError, BranchKind, HeadFacts, PushError, PushPlan, branch_prefix, classify,
    current_branch, plan_push,
};
use crate::test_support::sid;

const HEAD_SHA: &str = "1111111111111111111111111111111111111111";
const OTHER_SHA: &str = "2222222222222222222222222222222222222222";
const ZEROS: &str = "0000000000000000000000000000000000000000";

fn facts(current: Option<&str>, rebase_head: Option<&str>, containing: &[&str]) -> HeadFacts {
    HeadFacts {
        current: current.map(str::to_owned),
        rebase_head: rebase_head.map(str::to_owned),
        containing: containing.iter().map(|name| (*name).to_owned()).collect(),
        head: Some(HEAD_SHA.to_owned()),
    }
}

fn push_line(lref: &str, lsha: &str, rref: &str, rsha: &str) -> String {
    format!("{lref} {lsha} {rref} {rsha}\n")
}

fn work_item(id_text: &str) -> BranchKind {
    BranchKind::WorkItem(sid(id_text))
}

#[test]
fn it_should_read_the_branch_from_show_current_when_it_is_present() {
    let facts = facts(Some("feat/wi-w1__x"), None, &[]);

    let branch = current_branch(&facts);

    assert_eq!(branch, Ok("feat/wi-w1__x".to_owned()));
}

#[test]
fn it_should_prefer_show_current_over_the_rebase_head() {
    let facts = facts(
        Some("feat/wi-w1__x"),
        Some("refs/heads/feat/wi-w2__y\n"),
        &["feat/wi-w3__z"],
    );

    let branch = current_branch(&facts);

    assert_eq!(branch, Ok("feat/wi-w1__x".to_owned()));
}

#[test]
fn it_should_read_the_branch_from_the_rebase_head_when_show_current_is_empty() {
    let empty = facts(
        Some(""),
        Some("refs/heads/feat/wi-w1__x\n"),
        &["develop", "feat/wi-w2__y"],
    );
    let absent = facts(
        None,
        Some("refs/heads/feat/wi-w1__x\n"),
        &["develop", "feat/wi-w2__y"],
    );

    let from_empty = current_branch(&empty);
    let from_absent = current_branch(&absent);

    assert_eq!(from_empty, Ok("feat/wi-w1__x".to_owned()));
    assert_eq!(from_absent, Ok("feat/wi-w1__x".to_owned()));
}

#[test]
fn it_should_strip_refs_heads_and_the_trailing_newline_from_the_rebase_head() {
    let facts = facts(None, Some("refs/heads/feat/wi-w1__x\n"), &[]);

    let branch = current_branch(&facts);

    assert_eq!(branch, Ok("feat/wi-w1__x".to_owned()));
}

#[test]
fn it_should_use_the_only_branch_that_contains_head_when_it_is_detached() {
    let facts = facts(None, None, &["feat/wi-w1__x"]);

    let branch = current_branch(&facts);

    assert_eq!(branch, Ok("feat/wi-w1__x".to_owned()));
}

#[test]
fn it_should_report_ambiguity_when_several_branches_contain_head() {
    let facts = facts(None, None, &["develop", "feat/wi-w1__x"]);

    let branch = current_branch(&facts);

    assert_eq!(branch, Err(BranchError::Ambiguous));
}

#[test]
fn it_should_report_ambiguity_when_no_branch_contains_head() {
    let facts = facts(None, None, &[]);

    let branch = current_branch(&facts);

    assert_eq!(branch, Err(BranchError::Ambiguous));
}

#[test]
fn it_should_classify_main_as_main() {
    let kind = classify("main");

    assert_eq!(kind, BranchKind::Main);
}

#[test]
fn it_should_classify_develop_as_develop() {
    let kind = classify("develop");

    assert_eq!(kind, BranchKind::Develop);
}

#[test]
fn it_should_classify_a_type_slash_wi_id_double_underscore_slug_branch_as_a_work_item() {
    let kind = classify("feat/wi-w1__x");

    assert_eq!(kind, work_item("work-items.w1"));
}

#[test]
fn it_should_take_the_work_item_id_up_to_the_first_double_underscore() {
    let with_extra_slug = classify("feat/wi-w1__x__y");
    let front = classify("feat/wi-mg0__s5");

    assert_eq!(with_extra_slug, work_item("work-items.w1"));
    assert_eq!(front, work_item("work-items.mg0"));
}

#[test]
fn it_should_classify_a_work_item_branch_without_a_slug_as_other() {
    let without_separator = classify("feat/wi-w1");
    let with_empty_slug = classify("feat/wi-w1__");

    assert_eq!(without_separator, BranchKind::Other);
    assert_eq!(with_empty_slug, BranchKind::Other);
}

#[test]
fn it_should_classify_a_branch_without_a_type_prefix_as_other() {
    let kind = classify("wi-w1__x");

    assert_eq!(kind, BranchKind::Other);
}

#[test]
fn it_should_classify_a_branch_whose_id_is_not_a_valid_name_as_other() {
    let too_long = format!("feat/wi-{}__x", "a".repeat(64));
    let names = [
        "feat/wi-W1__x",
        "feat/wi-w.1__x",
        "feat/wi-w_1__x",
        "feat/wi-__x",
        "feat/wi--w1__x",
        too_long.as_str(),
    ];

    let kinds: Vec<_> = names.iter().map(|name| classify(name)).collect();

    assert_eq!(kinds, vec![BranchKind::Other; 6]);
}

#[test]
fn it_should_classify_a_branch_with_nested_prefixes_as_other() {
    let kind = classify("a/b/wi-w1__x");

    assert_eq!(kind, BranchKind::Other);
}

#[test]
fn it_should_not_treat_main_fix_as_main() {
    let kind = classify("main-fix");

    assert_eq!(kind, BranchKind::Other);
}

#[test]
fn it_should_not_treat_develop2_as_develop() {
    let kind = classify("develop2");

    assert_eq!(kind, BranchKind::Other);
}

#[test]
fn it_should_treat_a_rebase_head_without_the_refs_heads_prefix_as_absent() {
    let with_candidate = facts(None, Some("detached HEAD\n"), &["develop"]);
    let without_candidate = facts(None, Some("detached HEAD\n"), &[]);

    let from_candidate = current_branch(&with_candidate);
    let from_nothing = current_branch(&without_candidate);

    assert_eq!(from_candidate, Ok("develop".to_owned()));
    assert_eq!(from_nothing, Err(BranchError::Ambiguous));
}

#[test]
fn it_should_accept_a_slash_inside_the_slug() {
    let kind = classify("feat/wi-w1__a/b");

    assert_eq!(kind, work_item("work-items.w1"));
}

#[test]
fn it_should_bar_a_push_whose_remote_ref_is_main() {
    let lines = push_line("refs/heads/main", HEAD_SHA, "refs/heads/main", OTHER_SHA);

    let plan = plan_push(&lines, Some(HEAD_SHA), Some("main"));

    assert_eq!(
        plan,
        Err(PushError::ProtectedTarget {
            remote_ref: "refs/heads/main".to_owned()
        })
    );
}

#[test]
fn it_should_bar_a_push_that_deletes_main() {
    let lines = push_line("(delete)", ZEROS, "refs/heads/main", OTHER_SHA);

    let plan = plan_push(&lines, Some(HEAD_SHA), Some("feat/wi-w1__x"));

    assert_eq!(
        plan,
        Err(PushError::ProtectedTarget {
            remote_ref: "refs/heads/main".to_owned()
        })
    );
}

#[test]
fn it_should_skip_a_push_that_deletes_another_ref() {
    let lines = push_line("(delete)", ZEROS, "refs/heads/feat/old", OTHER_SHA);

    let plan = plan_push(&lines, Some(HEAD_SHA), Some("feat/wi-w1__x"));

    assert_eq!(plan, Ok(PushPlan::NothingToAttest));
}

#[test]
fn it_should_skip_a_pushed_tag() {
    let lines = push_line("refs/tags/v1", OTHER_SHA, "refs/tags/v1", ZEROS);

    let plan = plan_push(&lines, Some(HEAD_SHA), Some("feat/wi-w1__x"));

    assert_eq!(plan, Ok(PushPlan::NothingToAttest));
}

#[test]
fn it_should_check_head_when_every_pushed_ref_points_to_head() {
    let lines = push_line(
        "refs/heads/feat/wi-w1__x",
        HEAD_SHA,
        "refs/heads/feat/wi-w1__x",
        ZEROS,
    ) + &push_line("HEAD", HEAD_SHA, "refs/heads/feat/wi-w1__x2", ZEROS);

    let plan = plan_push(&lines, Some(HEAD_SHA), Some("feat/wi-w1__x"));

    assert_eq!(plan, Ok(PushPlan::CheckHead));
}

#[test]
fn it_should_refuse_a_pushed_ref_that_does_not_point_to_head() {
    let other_branch = push_line(
        "refs/heads/feat/wi-w2__y",
        OTHER_SHA,
        "refs/heads/feat/wi-w2__y",
        ZEROS,
    );
    let checkout_branch = push_line(
        "refs/heads/feat/wi-w1__x",
        HEAD_SHA,
        "refs/heads/feat/wi-w1__x",
        ZEROS,
    );

    let from_main_checkout = plan_push(&other_branch, Some(HEAD_SHA), Some("main"));
    let without_head = plan_push(&checkout_branch, None, Some("feat/wi-w1__x"));

    assert_eq!(
        from_main_checkout,
        Err(PushError::NotCheckedOut {
            local_ref: "refs/heads/feat/wi-w2__y".to_owned(),
            remote_ref: "refs/heads/feat/wi-w2__y".to_owned()
        })
    );
    assert_eq!(
        without_head,
        Err(PushError::NotCheckedOut {
            local_ref: "refs/heads/feat/wi-w1__x".to_owned(),
            remote_ref: "refs/heads/feat/wi-w1__x".to_owned()
        })
    );
}

#[test]
fn it_should_report_nothing_to_attest_when_a_push_only_deletes_refs_and_pushes_tags() {
    let lines = push_line("(delete)", ZEROS, "refs/heads/feat/old", OTHER_SHA)
        + &push_line("refs/tags/v1", OTHER_SHA, "refs/tags/v1", ZEROS);

    let plan = plan_push(&lines, Some(HEAD_SHA), Some("feat/wi-w1__x"));

    assert_eq!(plan, Ok(PushPlan::NothingToAttest));
}

#[test]
fn it_should_refuse_a_malformed_pre_push_line() {
    let two_fields = format!("refs/heads/feat/wi-w1__x {HEAD_SHA}\n");
    let blank_between_valid_lines = push_line(
        "refs/heads/feat/wi-w1__x",
        HEAD_SHA,
        "refs/heads/feat/wi-w1__x",
        ZEROS,
    ) + "\n"
        + &push_line("HEAD", HEAD_SHA, "refs/heads/feat/wi-w1__x2", ZEROS);

    let first = plan_push(&two_fields, Some(HEAD_SHA), Some("feat/wi-w1__x"));
    let second = plan_push(
        &blank_between_valid_lines,
        Some(HEAD_SHA),
        Some("feat/wi-w1__x"),
    );

    assert_eq!(first, Err(PushError::Malformed { line: 1 }));
    assert_eq!(second, Err(PushError::Malformed { line: 2 }));
}

#[test]
fn it_should_bar_main_even_when_another_line_is_malformed() {
    let lines =
        "garbage\n".to_owned() + &push_line("refs/heads/feat", HEAD_SHA, "refs/heads/main", ZEROS);

    let plan = plan_push(&lines, Some(HEAD_SHA), Some("feat"));

    assert_eq!(
        plan,
        Err(PushError::ProtectedTarget {
            remote_ref: "refs/heads/main".to_owned()
        })
    );
}

#[test]
fn it_should_report_a_malformed_line_before_a_pushed_ref_that_does_not_point_to_head() {
    let lines = push_line(
        "refs/heads/feat/wi-w2__y",
        OTHER_SHA,
        "refs/heads/feat/wi-w2__y",
        ZEROS,
    ) + "garbage\n";

    let plan = plan_push(&lines, Some(HEAD_SHA), Some("feat/wi-w1__x"));

    assert_eq!(plan, Err(PushError::Malformed { line: 2 }));
}

#[test]
fn it_should_report_nothing_to_attest_when_the_pre_push_input_is_empty() {
    let plan = plan_push("", Some(HEAD_SHA), Some("feat/wi-w1__x"));

    assert_eq!(plan, Ok(PushPlan::NothingToAttest));
}

#[test]
fn it_should_refuse_to_push_a_local_branch_other_than_the_checkout_even_when_it_points_to_head() {
    let lines = push_line(
        "refs/heads/feat/wi-w8__same",
        HEAD_SHA,
        "refs/heads/feat/wi-w8__same",
        ZEROS,
    );

    let plan = plan_push(&lines, Some(HEAD_SHA), Some("feat/wi-w1__x"));

    assert_eq!(
        plan,
        Err(PushError::NotCheckedOut {
            local_ref: "refs/heads/feat/wi-w8__same".to_owned(),
            remote_ref: "refs/heads/feat/wi-w8__same".to_owned()
        })
    );
}

#[test]
fn it_should_accept_the_checkout_branch_pushed_to_another_remote_ref() {
    let lines = push_line(
        "refs/heads/feat/wi-w1__x",
        HEAD_SHA,
        "refs/heads/feat/wi-w9__z",
        ZEROS,
    );

    let plan = plan_push(&lines, Some(HEAD_SHA), Some("feat/wi-w1__x"));

    assert_eq!(plan, Ok(PushPlan::CheckHead));
}

#[test]
fn it_should_accept_a_head_local_ref_in_a_push_whatever_the_checkout_branch() {
    let lines = push_line("HEAD", HEAD_SHA, "refs/heads/integ", ZEROS);

    let plan = plan_push(&lines, Some(HEAD_SHA), Some("feat/wi-w3__z"));

    assert_eq!(plan, Ok(PushPlan::CheckHead));
}

// Prefixo (tipo) da branch

#[test]
fn it_should_extract_the_type_of_each_of_the_seven_prefixes() {
    let types = [
        "feat", "fix", "refactor", "tests", "bugfix", "chore", "docs",
    ];

    let names: Vec<String> = types
        .iter()
        .map(|kind| format!("{kind}/wi-w1__x"))
        .collect();

    let prefixes: Vec<_> = names.iter().map(|name| branch_prefix(name)).collect();

    let expected: Vec<_> = types.iter().map(|kind| Some(*kind)).collect();
    assert_eq!(prefixes, expected);
}

#[test]
fn it_should_extract_a_type_outside_the_seven_prefixes_without_judging_it() {
    let prefix = branch_prefix("perf/wi-w1__x");

    assert_eq!(prefix, Some("perf"));
}

#[test]
fn it_should_extract_the_type_when_the_slug_has_a_double_underscore_or_a_slash() {
    let with_extra_slug = branch_prefix("feat/wi-w1__x__y");
    let with_slash = branch_prefix("fix/wi-w1__a/b");

    assert_eq!(with_extra_slug, Some("feat"));
    assert_eq!(with_slash, Some("fix"));
}

#[test]
fn it_should_give_no_prefix_to_main_and_develop() {
    let main = branch_prefix("main");
    let develop = branch_prefix("develop");

    assert_eq!(main, None);
    assert_eq!(develop, None);
}

#[test]
fn it_should_give_no_prefix_to_a_work_item_branch_without_a_slug() {
    let without_separator = branch_prefix("feat/wi-w1");
    let with_empty_slug = branch_prefix("feat/wi-w1__");

    assert_eq!(without_separator, None);
    assert_eq!(with_empty_slug, None);
}

#[test]
fn it_should_give_no_prefix_to_a_branch_without_a_type_or_with_nested_types() {
    let without_type = branch_prefix("wi-w1__x");
    let nested = branch_prefix("a/b/wi-w1__x");
    let empty_type = branch_prefix("/wi-w1__x");

    assert_eq!(without_type, None);
    assert_eq!(nested, None);
    assert_eq!(empty_type, None);
}

#[test]
fn it_should_give_no_prefix_when_the_id_is_not_a_valid_name() {
    let too_long = format!("feat/wi-{}__x", "a".repeat(64));
    let names = [
        "feat/wi-W1__x",
        "feat/wi-w.1__x",
        "feat/wi-__x",
        too_long.as_str(),
    ];

    let prefixes: Vec<_> = names.iter().map(|name| branch_prefix(name)).collect();

    assert_eq!(prefixes, vec![None; 4]);
}

#[test]
fn it_should_give_no_prefix_to_a_ref_shaped_or_option_shaped_name() {
    let names = ["pr:7", "-x", "--branch=-x", "$(id)", "a;touch x", ""];

    let prefixes: Vec<_> = names.iter().map(|name| branch_prefix(name)).collect();

    assert_eq!(prefixes, vec![None; 6]);
}
