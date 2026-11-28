use crate::workflow::shell::git_command::{BranchName, GitCommand, PushTarget, recognize};

fn created(name: &str) -> Vec<GitCommand> {
    vec![GitCommand::CreateBranch(BranchName::Literal(
        name.to_owned(),
    ))]
}

fn unknown_name() -> Vec<GitCommand> {
    vec![GitCommand::CreateBranch(BranchName::Unknown)]
}

fn pushed(targets: Vec<PushTarget>) -> Vec<GitCommand> {
    vec![GitCommand::Push(targets)]
}

fn branch(name: &str) -> PushTarget {
    PushTarget::Branch(name.to_owned())
}

fn deleted(name: &str) -> PushTarget {
    PushTarget::Delete(name.to_owned())
}

fn hits_main(command: &str) -> bool {
    recognize(command)
        .iter()
        .any(|recognized| match recognized {
            GitCommand::Push(targets) => targets.iter().any(|target| target.hits("main")),
            _ => false,
        })
}

// CA-008: criação de branch

#[test]
fn it_should_return_the_name_of_a_branch_created_by_switch_c() {
    assert_eq!(recognize("git switch -c x"), created("x"));
}

#[test]
fn it_should_return_the_name_of_a_branch_created_by_switch_capital_c() {
    assert_eq!(recognize("git switch -C x"), created("x"));
}

#[test]
fn it_should_return_the_name_of_a_branch_created_by_switch_create() {
    assert_eq!(recognize("git switch --create x"), created("x"));
}

#[test]
fn it_should_return_the_name_of_a_branch_created_by_switch_force_create() {
    assert_eq!(recognize("git switch --force-create x"), created("x"));
}

#[test]
fn it_should_return_the_name_of_a_branch_created_by_checkout_b() {
    assert_eq!(recognize("git checkout -b x"), created("x"));
}

#[test]
fn it_should_return_the_name_of_a_branch_created_by_checkout_capital_b() {
    assert_eq!(recognize("git checkout -B x"), created("x"));
}

#[test]
fn it_should_return_the_name_of_a_branch_created_by_git_branch() {
    assert_eq!(recognize("git branch x"), created("x"));
}

#[test]
fn it_should_return_the_name_of_a_branch_created_by_git_branch_with_a_start_point() {
    assert_eq!(recognize("git branch x origin/develop"), created("x"));
}

#[test]
fn it_should_return_the_name_of_a_branch_created_by_worktree_add_b() {
    assert_eq!(recognize("git worktree add -b x ../x"), created("x"));
}

#[test]
fn it_should_return_the_name_of_a_branch_created_by_worktree_add_capital_b_with_a_start_point() {
    assert_eq!(
        recognize("git worktree add -B x ../x develop"),
        created("x")
    );
}

#[test]
fn it_should_not_recognize_a_branch_deletion_as_a_creation() {
    assert_eq!(recognize("git branch -d x"), vec![]);
}

#[test]
fn it_should_not_recognize_a_branch_rename_as_a_creation() {
    assert_eq!(recognize("git branch -m a b"), vec![]);
}

#[test]
fn it_should_not_recognize_a_branch_listing_as_a_creation() {
    assert_eq!(recognize("git branch --list"), vec![]);
}

#[test]
fn it_should_not_recognize_setting_the_upstream_as_a_creation() {
    assert_eq!(recognize("git branch -u origin/x"), vec![]);
}

#[test]
fn it_should_not_recognize_a_plain_checkout_as_a_creation() {
    assert_eq!(recognize("git checkout x"), vec![]);
}

#[test]
fn it_should_not_recognize_a_plain_switch_as_a_creation() {
    assert_eq!(recognize("git switch x"), vec![]);
}

#[test]
fn it_should_not_recognize_gh_pr_checkout_as_a_creation() {
    assert_eq!(recognize("gh pr checkout 3"), vec![]);
}

#[test]
fn it_should_not_recognize_the_checkout_of_a_remote_branch_as_a_creation() {
    assert_eq!(recognize("git checkout origin/x"), vec![]);
}

#[test]
fn it_should_not_recognize_worktree_add_without_b_as_a_creation() {
    assert_eq!(recognize("git worktree add ../x"), vec![]);
}

#[test]
fn it_should_give_an_unknown_name_for_a_quoted_variable() {
    assert_eq!(recognize("git switch -c \"$NAME\""), unknown_name());
}

#[test]
fn it_should_give_an_unknown_name_for_a_command_substitution() {
    assert_eq!(recognize("git switch -c $(date +%s)"), unknown_name());
}

#[test]
fn it_should_give_an_unknown_name_for_a_backtick_substitution() {
    assert_eq!(recognize("git switch -c `date +%s`"), unknown_name());
}

#[test]
fn it_should_recognize_a_creation_after_git_dash_capital_c() {
    assert_eq!(recognize("git -C /r switch -c x"), created("x"));
}

#[test]
fn it_should_recognize_a_creation_after_git_dash_c_with_a_config_pair() {
    assert_eq!(recognize("git -c a=b switch -c x"), created("x"));
}

#[test]
fn it_should_recognize_a_creation_under_rtk_proxy() {
    assert_eq!(recognize("rtk proxy git switch -c x"), created("x"));
}

#[test]
fn it_should_recognize_a_creation_under_env() {
    assert_eq!(recognize("env A=1 git switch -c x"), created("x"));
}

#[test]
fn it_should_recognize_a_creation_after_cd_in_a_compound_line() {
    assert_eq!(recognize("cd /r && git switch -c x"), created("x"));
}

#[test]
fn it_should_not_recognize_a_creation_inside_bash_c() {
    assert_eq!(recognize("bash -c \"git switch -c x\""), vec![]);
}

#[test]
fn it_should_not_recognize_a_creation_inside_eval() {
    assert_eq!(recognize("eval \"git switch -c x\""), vec![]);
}

#[test]
fn it_should_not_recognize_a_creation_after_another_git_global_option() {
    assert_eq!(recognize("git --git-dir=/r/.git switch -c x"), vec![]);
}

// CA-009: push

#[test]
fn it_should_recognize_a_push_of_main_to_origin() {
    assert_eq!(
        recognize("git push origin main"),
        pushed(vec![branch("main")])
    );
}

#[test]
fn it_should_recognize_a_push_of_head_to_main() {
    assert_eq!(
        recognize("git push origin HEAD:main"),
        pushed(vec![branch("main")])
    );
}

#[test]
fn it_should_recognize_a_push_of_a_branch_to_main() {
    assert_eq!(
        recognize("git push origin feat:main"),
        pushed(vec![branch("main")])
    );
}

#[test]
fn it_should_normalize_refs_heads_in_the_destination_of_a_push() {
    assert_eq!(
        recognize("git push origin feat:refs/heads/main"),
        pushed(vec![branch("main")])
    );
}

#[test]
fn it_should_recognize_a_forced_push_of_a_branch_to_main() {
    assert_eq!(
        recognize("git push -f origin +feat:main"),
        pushed(vec![branch("main")])
    );
}

#[test]
fn it_should_recognize_the_deletion_of_main_by_an_empty_source() {
    assert_eq!(
        recognize("git push origin :main"),
        pushed(vec![deleted("main")])
    );
}

#[test]
fn it_should_recognize_the_deletion_of_main_by_the_delete_option() {
    assert_eq!(
        recognize("git push origin --delete main"),
        pushed(vec![deleted("main")])
    );
}

#[test]
fn it_should_recognize_a_push_of_all_branches() {
    assert_eq!(recognize("git push --all"), pushed(vec![PushTarget::All]));
}

#[test]
fn it_should_recognize_a_mirror_push() {
    assert_eq!(
        recognize("git push --mirror"),
        pushed(vec![PushTarget::Mirror])
    );
}

#[test]
fn it_should_recognize_a_push_without_refspec_as_the_current_branch() {
    assert_eq!(recognize("git push"), pushed(vec![PushTarget::Current]));
}

#[test]
fn it_should_not_recognize_a_dry_run_push() {
    assert!(!hits_main("git push --dry-run origin main"));
}

#[test]
fn it_should_not_take_a_push_of_another_branch_as_a_push_of_main() {
    assert!(!hits_main("git push origin feat/x"));
}

#[test]
fn it_should_hit_any_branch_with_all_and_mirror() {
    assert!(PushTarget::All.hits("main") && PushTarget::Mirror.hits("trunk"));
}

#[test]
fn it_should_never_hit_a_branch_with_the_current_target() {
    assert!(!PushTarget::Current.hits("main"));
}

// CA-009: merge pelo provedor

#[test]
fn it_should_recognize_gh_pr_merge_with_a_number() {
    assert_eq!(recognize("gh pr merge 3"), vec![GitCommand::ProviderMerge]);
}

#[test]
fn it_should_recognize_gh_pr_merge_with_a_strategy() {
    assert_eq!(
        recognize("gh pr merge --squash"),
        vec![GitCommand::ProviderMerge]
    );
}

#[test]
fn it_should_recognize_gh_pr_merge_under_rtk_proxy() {
    assert_eq!(
        recognize("rtk proxy gh pr merge 3"),
        vec![GitCommand::ProviderMerge]
    );
}

#[test]
fn it_should_recognize_gh_pr_merge_after_another_command() {
    assert_eq!(
        recognize("echo ok && gh pr merge"),
        vec![GitCommand::ProviderMerge]
    );
}

#[test]
fn it_should_not_recognize_gh_pr_view_as_a_merge() {
    assert_eq!(recognize("gh pr view 3"), vec![]);
}

#[test]
fn it_should_not_recognize_gh_pr_merge_queue_as_a_merge() {
    assert_eq!(recognize("gh pr merge-queue"), vec![]);
}
