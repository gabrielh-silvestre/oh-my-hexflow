use crate::adapters::text::{CommandKey, deferred, renderer_for};

#[test]
fn it_should_give_a_renderer_to_exactly_the_commands_that_are_not_deferred() {
    let mismatched: Vec<CommandKey> = CommandKey::ALL
        .into_iter()
        .filter(|key| renderer_for(*key).is_none() != deferred(*key))
        .collect();

    assert_eq!(mismatched, []);
}

#[test]
fn it_should_defer_exactly_the_machine_read_commands() {
    let expected = [
        CommandKey::GateWorktree,
        CommandKey::HooksPostCommit,
        CommandKey::HooksPostMerge,
        CommandKey::HookSessionStart,
        CommandKey::HookSpecLock,
        CommandKey::HookAgentRole,
        CommandKey::HookSubagentStart,
        CommandKey::HookBranchCreated,
        CommandKey::HookBranchBirth,
        CommandKey::HookCommandGuard,
        CommandKey::HookMcpMergeGuard,
        CommandKey::HookMcpWriteWarning,
    ];

    let deferred_keys: Vec<CommandKey> = CommandKey::ALL
        .into_iter()
        .filter(|key| deferred(*key))
        .collect();

    assert_eq!(deferred_keys.len(), expected.len(), "{deferred_keys:?}");
    assert!(
        expected.iter().all(|key| deferred_keys.contains(key)),
        "{deferred_keys:?}"
    );
}

#[test]
fn it_should_give_a_renderer_to_twenty_four_of_the_thirty_six_commands() {
    let with_renderer = CommandKey::ALL
        .into_iter()
        .filter(|key| renderer_for(*key).is_some())
        .count();

    assert_eq!((with_renderer, CommandKey::ALL.len()), (24, 36));
}

#[test]
fn it_should_give_merge_its_own_renderer_under_the_name_merge() {
    let key = CommandKey::Merge;

    let rendered = (renderer_for(key).is_some(), deferred(key));

    assert_eq!(rendered, (true, false));
}
