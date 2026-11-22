const FOLDER: &str = "wi-w1__login";
const DEFAULT_SPECS: &str = ".omhl/specs";
const CUSTOM_SPECS: &str = "docs/team-specs";
const FROZEN: &str = "spec-frozen";
const IN_WORKTREE: &str = "spec-in-linked-worktree";

mod lock {
    use std::path::Path;

    use crate::workflow::lock::{
        Cause, Facts, SpecPath, Tool, Verdict, decide, next_amendment, parse_spec_path, unavailable,
    };

    use super::{CUSTOM_SPECS, DEFAULT_SPECS, FOLDER, FROZEN, IN_WORKTREE};

    const MAIN_FOLDER: &str = "/main/.omhl/specs/wi-w1__login";

    // Camada pura: `crate::workflow::lock::{parse_spec_path, next_amendment, decide, Tool}`

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    fn spec_path(work_item: &str, folder: &str, file: &str) -> SpecPath {
        SpecPath {
            work_item: work_item.to_owned(),
            folder: folder.to_owned(),
            file: file.to_owned(),
        }
    }

    fn verdict(tool: Tool, file: &str, existing: &[&str], frozen: bool, linked: bool) -> Verdict {
        verdict_with(tool, file, existing, frozen, false, linked)
    }

    fn verdict_with(
        tool: Tool,
        file: &str,
        existing: &[&str],
        has_branch: bool,
        marked: bool,
        linked: bool,
    ) -> Verdict {
        let files = strings(existing);
        let facts = Facts {
            has_branch,
            marked,
            linked_worktree: linked,
            folder_exists: true,
            file_exists: existing.contains(&file),
            folder_files: &files,
            main_folder: Path::new(MAIN_FOLDER),
        };

        decide(tool, &spec_path("w1", FOLDER, file), &facts)
    }

    fn denial(verdict: Verdict) -> (&'static str, String) {
        match verdict {
            Verdict::Deny { code, reason } => (code, reason),
            Verdict::Allow => panic!("expected a denial, got Allow"),
        }
    }

    #[test]
    fn it_should_parse_the_work_item_folder_and_file_of_a_path_under_the_default_specs_dir() {
        let parsed = parse_spec_path(
            DEFAULT_SPECS,
            Path::new("/repo/.omhl/specs/wi-w1__login/parte-01.md"),
        );

        assert_eq!(parsed, Some(spec_path("w1", FOLDER, "parte-01.md")));
    }

    #[test]
    fn it_should_parse_a_path_in_a_linked_worktree_the_same_way() {
        let parsed = parse_spec_path(
            DEFAULT_SPECS,
            Path::new("/scratch/wt/.omhl/specs/wi-w1__login/parte-01.md"),
        );

        assert_eq!(parsed, Some(spec_path("w1", FOLDER, "parte-01.md")));
    }

    #[test]
    fn it_should_parse_a_path_under_a_custom_specs_dir() {
        let parsed = parse_spec_path(
            CUSTOM_SPECS,
            Path::new("/repo/docs/team-specs/wi-w1__login/parte-01.md"),
        );

        assert_eq!(parsed, Some(spec_path("w1", FOLDER, "parte-01.md")));
    }

    #[test]
    fn it_should_not_parse_the_default_dir_when_the_specs_dir_is_customized() {
        let parsed = parse_spec_path(
            CUSTOM_SPECS,
            Path::new("/repo/.omhl/specs/wi-w1__login/parte-01.md"),
        );

        assert_eq!(parsed, None);
    }

    #[test]
    fn it_should_not_parse_a_path_outside_the_specs_dir() {
        let parsed = parse_spec_path(DEFAULT_SPECS, Path::new("/repo/src/main.rs"));

        assert_eq!(parsed, None);
    }

    #[test]
    fn it_should_not_parse_a_work_item_folder_outside_the_specs_dir() {
        let parsed = parse_spec_path(
            DEFAULT_SPECS,
            Path::new("/repo/docs/wi-w1__login/parte-01.md"),
        );

        assert_eq!(parsed, None);
    }

    #[test]
    fn it_should_not_parse_a_file_directly_under_the_specs_dir() {
        let parsed = parse_spec_path(DEFAULT_SPECS, Path::new("/repo/.omhl/specs/notes.md"));

        assert_eq!(parsed, None);
    }

    #[test]
    fn it_should_not_parse_a_folder_without_the_work_item_segment() {
        let parsed = parse_spec_path(
            DEFAULT_SPECS,
            Path::new("/repo/.omhl/specs/drafts/parte-01.md"),
        );

        assert_eq!(parsed, None);
    }

    #[test]
    fn it_should_not_parse_a_folder_without_the_slug_separator() {
        let parsed = parse_spec_path(
            DEFAULT_SPECS,
            Path::new("/repo/.omhl/specs/wi-w1/parte-01.md"),
        );

        assert_eq!(parsed, None);
    }

    #[test]
    fn it_should_not_parse_a_folder_with_an_empty_slug() {
        let parsed = parse_spec_path(
            DEFAULT_SPECS,
            Path::new("/repo/.omhl/specs/wi-w1__/parte-01.md"),
        );

        assert_eq!(parsed, None);
    }

    #[test]
    fn it_should_not_parse_a_folder_whose_id_is_not_a_valid_name() {
        let parsed = parse_spec_path(
            DEFAULT_SPECS,
            Path::new("/repo/.omhl/specs/wi-W1__login/parte-01.md"),
        );

        assert_eq!(parsed, None);
    }

    #[test]
    fn it_should_not_parse_the_folder_path_itself() {
        let parsed = parse_spec_path(DEFAULT_SPECS, Path::new("/repo/.omhl/specs/wi-w1__login"));

        assert_eq!(parsed, None);
    }

    #[test]
    fn it_should_parse_a_path_that_climbs_out_and_back_into_the_specs_dir() {
        let parsed = parse_spec_path(
            DEFAULT_SPECS,
            Path::new("/repo/.omhl/specs/../specs/wi-w1__login/./parte-01.md"),
        );

        assert_eq!(parsed, Some(spec_path("w1", FOLDER, "parte-01.md")));
    }

    #[test]
    fn it_should_not_parse_a_path_that_climbs_out_of_the_specs_dir() {
        let parsed = parse_spec_path(
            DEFAULT_SPECS,
            Path::new("/repo/.omhl/specs/wi-w1__login/../../../src/parte-01.md"),
        );

        assert_eq!(parsed, None);
    }

    #[test]
    fn it_should_name_the_first_amendment_when_the_folder_has_none() {
        let next = next_amendment(&strings(&["parte-01.md", "parte-02.md"]));

        assert_eq!(next, "emenda-01.md");
    }

    #[test]
    fn it_should_name_the_first_amendment_for_an_empty_folder() {
        let next = next_amendment(&[]);

        assert_eq!(next, "emenda-01.md");
    }

    #[test]
    fn it_should_name_the_amendment_after_the_only_one() {
        let next = next_amendment(&strings(&["parte-01.md", "emenda-01.md"]));

        assert_eq!(next, "emenda-02.md");
    }

    #[test]
    fn it_should_name_the_amendment_after_the_highest_number_even_with_a_gap() {
        let next = next_amendment(&strings(&["emenda-01.md", "emenda-03.md"]));

        assert_eq!(next, "emenda-04.md");
    }

    #[test]
    fn it_should_name_the_next_amendment_whatever_the_listing_order() {
        let next = next_amendment(&strings(&["emenda-02.md", "emenda-01.md"]));

        assert_eq!(next, "emenda-03.md");
    }

    #[test]
    fn it_should_ignore_files_that_are_not_two_digit_amendments_when_numbering() {
        let files = strings(&[
            "emenda-1.md",
            "emenda-x1.md",
            "emenda-07.txt",
            "emenda-.md",
            "notes.md",
        ]);

        let next = next_amendment(&files);

        assert_eq!(next, "emenda-01.md");
    }

    #[test]
    fn it_should_parse_only_the_three_file_editing_tools() {
        let parsed: Vec<Option<Tool>> = ["Write", "Edit", "MultiEdit", "Read", "Bash", "write"]
            .into_iter()
            .map(Tool::parse)
            .collect();

        assert_eq!(
            parsed,
            vec![
                Some(Tool::Write),
                Some(Tool::Edit),
                Some(Tool::MultiEdit),
                None,
                None,
                None,
            ]
        );
    }

    #[test]
    fn it_should_allow_any_tool_when_the_work_item_has_no_branch() {
        let verdicts: Vec<Verdict> = [Tool::Write, Tool::Edit, Tool::MultiEdit]
            .into_iter()
            .map(|tool| verdict(tool, "parte-01.md", &["parte-01.md"], false, false))
            .collect();

        assert_eq!(verdicts, vec![Verdict::Allow; 3]);
    }

    #[test]
    fn it_should_deny_edit_on_an_existing_file_when_the_work_item_has_a_branch() {
        let (code, _) = denial(verdict(
            Tool::Edit,
            "parte-01.md",
            &["parte-01.md"],
            true,
            false,
        ));

        assert_eq!(code, FROZEN);
    }

    #[test]
    fn it_should_deny_multi_edit_on_an_existing_file_when_the_work_item_has_a_branch() {
        let (code, _) = denial(verdict(
            Tool::MultiEdit,
            "parte-01.md",
            &["parte-01.md"],
            true,
            false,
        ));

        assert_eq!(code, FROZEN);
    }

    #[test]
    fn it_should_deny_write_over_an_existing_file_when_the_work_item_has_a_branch() {
        let (code, _) = denial(verdict(
            Tool::Write,
            "parte-01.md",
            &["parte-01.md"],
            true,
            false,
        ));

        assert_eq!(code, FROZEN);
    }

    #[test]
    fn it_should_deny_a_new_part_when_the_work_item_has_a_branch() {
        let (code, _) = denial(verdict(
            Tool::Write,
            "parte-02.md",
            &["parte-01.md"],
            true,
            false,
        ));

        assert_eq!(code, FROZEN);
    }

    #[test]
    fn it_should_allow_the_first_amendment_when_the_work_item_has_a_branch() {
        let verdict = verdict(Tool::Write, "emenda-01.md", &["parte-01.md"], true, false);

        assert_eq!(verdict, Verdict::Allow);
    }

    #[test]
    fn it_should_allow_the_amendment_after_the_highest_existing_one() {
        let verdict = verdict(
            Tool::Write,
            "emenda-04.md",
            &["parte-01.md", "emenda-01.md", "emenda-03.md"],
            true,
            false,
        );

        assert_eq!(verdict, Verdict::Allow);
    }

    #[test]
    fn it_should_deny_an_amendment_that_skips_a_number() {
        let (code, _) = denial(verdict(
            Tool::Write,
            "emenda-03.md",
            &["parte-01.md", "emenda-01.md"],
            true,
            false,
        ));

        assert_eq!(code, FROZEN);
    }

    #[test]
    fn it_should_deny_an_amendment_that_reuses_a_lower_number() {
        let (code, _) = denial(verdict(
            Tool::Write,
            "emenda-02.md",
            &["parte-01.md", "emenda-01.md", "emenda-03.md"],
            true,
            false,
        ));

        assert_eq!(code, FROZEN);
    }

    #[test]
    fn it_should_deny_write_over_an_existing_amendment() {
        let (code, _) = denial(verdict(
            Tool::Write,
            "emenda-01.md",
            &["parte-01.md", "emenda-01.md"],
            true,
            false,
        ));

        assert_eq!(code, FROZEN);
    }

    #[test]
    fn it_should_deny_edit_on_an_existing_amendment() {
        let (code, _) = denial(verdict(
            Tool::Edit,
            "emenda-01.md",
            &["parte-01.md", "emenda-01.md"],
            true,
            false,
        ));

        assert_eq!(code, FROZEN);
    }

    #[test]
    fn it_should_deny_a_new_amendment_whose_number_has_one_digit() {
        let (code, _) = denial(verdict(
            Tool::Write,
            "emenda-1.md",
            &["parte-01.md"],
            true,
            false,
        ));

        assert_eq!(code, FROZEN);
    }

    #[test]
    fn it_should_put_the_main_folder_and_the_next_amendment_path_in_the_frozen_reason() {
        let (_, reason) = denial(verdict(
            Tool::Edit,
            "parte-01.md",
            &["parte-01.md", "emenda-01.md"],
            true,
            false,
        ));

        assert!(reason.contains(MAIN_FOLDER), "reason: {reason}");
        assert!(
            reason.contains(&format!("{MAIN_FOLDER}/emenda-02.md")),
            "reason: {reason}"
        );
    }

    #[test]
    fn it_should_put_the_code_in_the_frozen_reason() {
        let (_, reason) = denial(verdict(
            Tool::Edit,
            "parte-01.md",
            &["parte-01.md"],
            true,
            false,
        ));

        assert!(reason.contains(FROZEN), "reason: {reason}");
    }

    #[test]
    fn it_should_deny_a_spec_in_a_linked_worktree_even_without_a_branch() {
        let (code, _) = denial(verdict(Tool::Write, "parte-01.md", &[], false, true));

        assert_eq!(code, IN_WORKTREE);
    }

    #[test]
    fn it_should_point_to_the_main_folder_when_denying_a_spec_in_a_linked_worktree() {
        let (_, reason) = denial(verdict(Tool::Write, "parte-01.md", &[], false, true));

        assert!(reason.contains(MAIN_FOLDER), "reason: {reason}");
        assert!(reason.contains(IN_WORKTREE), "reason: {reason}");
    }

    #[test]
    fn it_should_deny_a_spec_file_in_a_folder_that_does_not_exist_when_frozen() {
        let facts = Facts {
            has_branch: true,
            marked: false,
            linked_worktree: false,
            folder_exists: false,
            file_exists: false,
            folder_files: &[],
            main_folder: Path::new(MAIN_FOLDER),
        };

        let (code, reason) = denial(decide(
            Tool::Write,
            &spec_path("w1", FOLDER, "emenda-01.md"),
            &facts,
        ));

        assert_eq!(code, "spec-folder-unknown");
        assert!(reason.contains(MAIN_FOLDER), "reason: {reason}");
    }

    #[test]
    fn it_should_deny_when_the_work_item_is_marked_without_a_branch() {
        let verdict = verdict_with(
            Tool::Edit,
            "parte-01.md",
            &["parte-01.md", ".frozen"],
            false,
            true,
            false,
        );

        assert_eq!(denial(verdict).0, FROZEN);
    }

    #[test]
    fn it_should_allow_when_neither_branch_nor_mark_exists() {
        let verdict = verdict_with(
            Tool::Edit,
            "parte-01.md",
            &["parte-01.md"],
            false,
            false,
            false,
        );

        assert_eq!(verdict, Verdict::Allow);
    }

    #[test]
    fn it_should_deny_an_edit_of_the_mark_itself() {
        let verdict = verdict_with(
            Tool::Edit,
            ".frozen",
            &["parte-01.md", ".frozen"],
            false,
            true,
            false,
        );

        assert_eq!(denial(verdict).0, FROZEN);
    }

    #[test]
    fn it_should_allow_a_new_file_named_like_a_part_inside_a_subfolder() {
        let verdict = verdict(
            Tool::Write,
            "sub/parte-01.md",
            &["parte-01.md"],
            true,
            false,
        );

        assert_eq!(verdict, Verdict::Allow);
    }

    #[test]
    fn it_should_allow_a_new_file_named_like_an_amendment_inside_a_subfolder() {
        let verdict = verdict(
            Tool::Write,
            "sub/emenda-07.md",
            &["parte-01.md"],
            true,
            false,
        );

        assert_eq!(verdict, Verdict::Allow);
    }

    // Marcador do projeto dentro da pasta congelada (b5)

    #[test]
    fn it_should_deny_a_marker_file_written_in_a_frozen_folder_with_its_own_reason() {
        let verdict = verdict(Tool::Write, ".omhl.toml", &["parte-01.md"], true, false);

        let (code, reason) = denial(verdict);

        assert_eq!(code, FROZEN);
        assert!(reason.contains("project marker files"), "reason: {reason}");
    }

    #[test]
    fn it_should_deny_a_marker_file_at_any_depth_of_a_frozen_folder() {
        let verdict = verdict(Tool::Write, "sub/.omhl.toml", &["parte-01.md"], true, false);

        assert_eq!(denial(verdict).0, FROZEN);
    }

    #[test]
    fn it_should_deny_a_file_under_an_omhl_folder_of_a_frozen_folder() {
        let verdict = verdict(
            Tool::Write,
            ".omhl/workflow.toml",
            &["parte-01.md"],
            true,
            false,
        );

        assert_eq!(denial(verdict).0, FROZEN);
    }

    #[test]
    fn it_should_deny_an_edit_of_an_existing_marker_file_in_a_frozen_folder() {
        let verdict = verdict(
            Tool::Edit,
            ".omhl.toml",
            &["parte-01.md", ".omhl.toml"],
            true,
            false,
        );

        assert_eq!(denial(verdict).0, FROZEN);
    }

    #[test]
    fn it_should_allow_a_new_plain_file_in_a_frozen_folder() {
        let verdict = verdict(Tool::Write, "notas.md", &["parte-01.md"], true, false);

        assert_eq!(verdict, Verdict::Allow);
    }

    #[test]
    fn it_should_not_treat_a_name_that_only_resembles_the_marker_as_a_marker() {
        let verdict = verdict(
            Tool::Write,
            "notas.omhl.toml",
            &["parte-01.md"],
            true,
            false,
        );

        assert_eq!(verdict, Verdict::Allow);
    }

    #[test]
    fn it_should_allow_a_marker_file_in_a_folder_of_a_work_item_without_branch_or_mark() {
        let verdict = verdict(Tool::Write, ".omhl.toml", &["parte-01.md"], false, false);

        assert_eq!(verdict, Verdict::Allow);
    }

    // Negação por falha interna depois do reconhecimento

    const SUBJECT: &str = "wi-w1";
    const FIX_CLOSING: &str = "the edit stays denied until this is fixed";

    #[test]
    fn it_should_build_the_unavailable_denial_on_one_line_with_the_prefix_the_work_item_and_the_step()
     {
        let verdict = unavailable(
            SUBJECT,
            "checking the main directory",
            &Cause::Git("fatal: bad config line 1 in file .git/config".to_owned()),
        );

        let (code, reason) = denial(verdict);

        assert_eq!(code, "spec-lock-unavailable");
        assert!(
            reason.starts_with("spec-lock-unavailable: "),
            "reason: {reason}"
        );
        assert!(reason.contains("wi-w1"), "reason: {reason}");
        assert!(
            reason.contains("checking the main directory"),
            "reason: {reason}"
        );
        assert!(reason.contains("bad config line"), "reason: {reason}");
        assert!(!reason.contains('\n'), "reason: {reason}");
    }

    #[test]
    fn it_should_close_a_git_cause_with_until_this_is_fixed() {
        let verdict = unavailable(
            SUBJECT,
            "listing the local branches",
            &Cause::Git("x".to_owned()),
        );

        assert!(denial(verdict).1.ends_with(FIX_CLOSING));
    }

    #[test]
    fn it_should_close_a_read_cause_with_until_this_is_fixed_and_say_the_fixed_reason() {
        let verdict = unavailable(
            SUBJECT,
            "listing the files of the spec folder",
            &Cause::Read("the specs folder has too many entries"),
        );

        let (_, reason) = denial(verdict);

        assert!(reason.contains("the specs folder has too many entries"));
        assert!(reason.ends_with(FIX_CLOSING), "reason: {reason}");
    }

    #[test]
    fn it_should_close_a_timeout_with_retry_it_and_name_it() {
        let verdict = unavailable(SUBJECT, "checking the checkout", &Cause::GitTimedOut);

        let (_, reason) = denial(verdict);

        assert!(reason.contains("git timed out"), "reason: {reason}");
        assert!(
            reason.ends_with("the edit stays denied; retry it"),
            "reason: {reason}"
        );
    }

    #[test]
    fn it_should_turn_control_characters_of_the_git_message_into_spaces() {
        let verdict = unavailable(
            SUBJECT,
            "checking the main directory",
            &Cause::Git("fatal: bad value\n\"x\"\r\tnext".to_owned()),
        );

        let (_, reason) = denial(verdict);

        assert!(!reason.contains(['\n', '\r', '\t']), "reason: {reason:?}");
        assert!(
            reason.contains("bad value \"x\"  next"),
            "reason: {reason:?}"
        );
    }

    #[test]
    fn it_should_cut_the_git_message_at_200_characters() {
        let message = "Z".repeat(500);

        let verdict = unavailable(SUBJECT, "checking the main directory", &Cause::Git(message));

        assert_eq!(denial(verdict).1.matches('Z').count(), 200);
    }

    #[test]
    fn it_should_cut_a_long_subject_at_200_characters() {
        let subject = "Q".repeat(500);

        let verdict = unavailable(&subject, "reading the new content", &Cause::Read("fixed"));

        assert_eq!(denial(verdict).1.matches('Q').count(), 200);
    }

    #[test]
    fn it_should_say_so_when_git_fails_without_a_message() {
        let verdict = unavailable(
            SUBJECT,
            "checking the main directory",
            &Cause::Git(String::new()),
        );

        assert!(denial(verdict).1.contains("git failed without a message"));
    }
}

mod specs_dir_change {
    use std::path::Path;

    use crate::project::layout::EXT_FILE_MAX_BYTES;
    use crate::workflow::lock::{
        EditInput, Replacement, Verdict, is_same_path, is_workflow_candidate, resulting_texts,
        spec_dir_frozen, specs_dir_of,
    };

    const DEFAULT_DIR: &str = ".omhl/specs";
    const CUSTOM_WORKFLOW: &str = "version = 1\n\n[specs]\ndir = \"x\"\n";

    fn replacement(old: &str, new: &str, replace_all: bool) -> Replacement {
        Replacement {
            old: old.to_owned(),
            new: new.to_owned(),
            replace_all,
        }
    }

    fn edit(old: &str, new: &str, replace_all: bool) -> EditInput {
        EditInput::Edit(replacement(old, new, replace_all))
    }

    fn texts(input: &EditInput, current: &str) -> Option<Vec<String>> {
        resulting_texts(input, Some(current))
    }

    fn reason(verdict: Verdict) -> String {
        match verdict {
            Verdict::Deny { code, reason } => {
                assert_eq!(code, "spec-dir-frozen");
                reason
            }
            Verdict::Allow => panic!("expected a denial"),
        }
    }

    // `specs_dir_of`

    #[test]
    fn it_should_use_the_default_specs_dir_when_the_file_is_absent() {
        assert_eq!(specs_dir_of(None), DEFAULT_DIR);
    }

    #[test]
    fn it_should_use_the_default_specs_dir_when_a_valid_file_has_no_key() {
        let dir = specs_dir_of(Some(b"version = 1\n\n[specs]\nmax_lines = 10\n"));

        assert_eq!(dir, DEFAULT_DIR);
    }

    #[test]
    fn it_should_use_the_specs_dir_of_a_valid_file() {
        assert_eq!(specs_dir_of(Some(CUSTOM_WORKFLOW.as_bytes())), "x");
    }

    #[test]
    fn it_should_treat_a_dot_slash_specs_dir_as_the_same_folder() {
        assert!(is_same_path(
            Path::new("./docs/specs"),
            Path::new("docs/specs")
        ));
    }

    #[test]
    fn it_should_tell_two_different_specs_dirs_apart() {
        assert!(!is_same_path(
            Path::new("docs/specs"),
            Path::new(DEFAULT_DIR)
        ));
    }

    #[test]
    fn it_should_use_the_default_specs_dir_when_the_file_is_refused_by_parse_workflow() {
        assert_eq!(specs_dir_of(Some(b"[specs")), DEFAULT_DIR);
    }

    #[test]
    fn it_should_use_the_default_specs_dir_when_the_file_is_not_utf8() {
        assert_eq!(specs_dir_of(Some(&[0xff, 0xfe, 0x00])), DEFAULT_DIR);
    }

    #[test]
    fn it_should_cut_the_file_above_the_size_limit_to_the_default_specs_dir_and_keep_the_limit_itself()
     {
        let padding = |extra: usize| {
            let size = EXT_FILE_MAX_BYTES as usize + extra - CUSTOM_WORKFLOW.len();

            format!("{CUSTOM_WORKFLOW}{}", "#".repeat(size))
        };

        let at_limit = specs_dir_of(Some(padding(0).as_bytes()));
        let above_limit = specs_dir_of(Some(padding(1).as_bytes()));

        assert_eq!(
            (at_limit.as_str(), above_limit.as_str()),
            ("x", DEFAULT_DIR)
        );
    }

    // Filtro lexical

    #[test]
    fn it_should_match_the_workflow_file_by_its_last_two_normalized_names() {
        for path in ["/p/.omhl/workflow.toml", "/p/.omhl/specs/../workflow.toml"] {
            assert!(is_workflow_candidate(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn it_should_not_match_a_workflow_file_outside_the_omhl_folder() {
        for path in ["/p/.omhl/specs/workflow.toml", "/p/omhl/workflow.toml"] {
            assert!(!is_workflow_candidate(Path::new(path)), "{path}");
        }
    }

    // Conteúdo resultante

    #[test]
    fn it_should_take_the_content_of_a_write_as_it_is() {
        let input = EditInput::Write {
            content: "novo".to_owned(),
        };

        assert_eq!(resulting_texts(&input, None), Some(vec!["novo".to_owned()]));
    }

    #[test]
    fn it_should_replace_every_occurrence_with_replace_all() {
        let result = texts(&edit("a", "b", true), "a-a");

        assert_eq!(result, Some(vec!["b-b".to_owned(); 3]));
    }

    #[test]
    fn it_should_replace_only_the_first_occurrence_without_replace_all() {
        let result = texts(&edit("a", "b", false), "a-a");

        assert_eq!(result, Some(vec!["b-a".to_owned(); 3]));
    }

    #[test]
    fn it_should_apply_a_multi_edit_in_order() {
        let input = EditInput::MultiEdit(vec![
            replacement("a", "b", false),
            replacement("b", "c", false),
        ]);

        assert_eq!(texts(&input, "a"), Some(vec!["c".to_owned(); 3]));
    }

    #[test]
    fn it_should_fail_when_the_old_string_is_missing_from_the_text() {
        assert_eq!(texts(&edit("zzz", "b", false), "a"), None);
    }

    #[test]
    fn it_should_fail_when_the_old_string_is_empty() {
        assert_eq!(texts(&edit("", "b", false), "a"), None);
    }

    #[test]
    fn it_should_fail_when_the_payload_field_is_missing() {
        assert_eq!(texts(&EditInput::Unreadable, "a"), None);
    }

    #[test]
    fn it_should_fail_when_an_edit_has_no_current_text() {
        assert_eq!(resulting_texts(&edit("a", "b", false), None), None);
    }

    #[test]
    fn it_should_apply_an_old_string_with_a_line_feed_to_the_normalized_text() {
        let result = texts(
            &edit("\"a\"\nmax", "\"b\"\nmax", false),
            "dir = \"a\"\r\nmax = 1\r\n",
        );

        assert_eq!(result, Some(vec!["dir = \"b\"\nmax = 1\n".to_owned(); 2]));
    }

    #[test]
    fn it_should_replace_different_occurrences_in_each_version_with_replace_all() {
        let result = texts(&edit("x\n", "y\n", true), "x\r\nx\nx");

        assert_eq!(
            result,
            Some(vec![
                "x\r\ny\nx".to_owned(),
                "y\ny\nx".to_owned(),
                "y\ny\nx".to_owned(),
            ])
        );
    }

    #[test]
    fn it_should_apply_strings_with_a_carriage_return_only_in_the_third_version() {
        let result = texts(&edit("a\r\nb", "c\r\nd", false), "a\nb");

        assert_eq!(result, Some(vec!["c\nd".to_owned()]));
    }

    #[test]
    fn it_should_fail_a_multi_edit_whose_first_edit_fits_only_the_raw_text_and_second_only_the_normalized()
     {
        let input = EditInput::MultiEdit(vec![
            replacement("a\r", "X", false),
            replacement("b\nc", "Y", false),
        ]);

        assert_eq!(texts(&input, "a\r\nb\r\nc"), None);
    }

    // Veredito

    #[test]
    fn it_should_build_the_spec_dir_frozen_denial_on_one_line_naming_path_dirs_and_work_item() {
        let verdict = spec_dir_frozen(
            Path::new("/p/.omhl/workflow.toml"),
            ".omhl/specs",
            "docs/specs",
            "wi-w1",
        );

        let reason = reason(verdict);

        assert!(reason.starts_with("spec-dir-frozen: "), "reason: {reason}");
        assert!(!reason.contains('\n'), "reason: {reason}");
        for fact in [
            "/p/.omhl/workflow.toml",
            "\".omhl/specs\"",
            "\"docs/specs\"",
            "wi-w1",
        ] {
            assert!(reason.contains(fact), "{fact}: {reason}");
        }
    }

    #[test]
    fn it_should_end_the_spec_dir_frozen_denial_asking_the_owner_without_naming_a_way_around() {
        let verdict = spec_dir_frozen(Path::new("/p/.omhl/workflow.toml"), "a", "b", "wi-w1");

        let reason = reason(verdict);

        assert!(reason.contains("outside Claude Code"), "reason: {reason}");
        assert!(
            reason.ends_with("ask the owner and do not change it through any tool"),
            "reason: {reason}"
        );
        assert!(!reason.contains("Bash"), "reason: {reason}");
        assert!(!reason.contains("terminal"), "reason: {reason}");
    }

    #[test]
    fn it_should_cut_the_values_of_the_spec_dir_frozen_denial_at_200_characters() {
        let long = "Q".repeat(500);

        let verdict = spec_dir_frozen(Path::new("/p/\n.omhl/workflow.toml"), &long, &long, &long);

        let reason = reason(verdict);
        assert_eq!(reason.matches('Q').count(), 600);
        assert!(!reason.contains('\n'), "reason: {reason:?}");
    }
}

mod git_dir {
    use std::path::Path;

    use crate::workflow::lock::{Verdict, git_dir_protected, has_git_component};

    fn matches(path: &str) -> bool {
        has_git_component(Path::new(path), Path::new("/"))
    }

    fn reason(verdict: Verdict) -> String {
        match verdict {
            Verdict::Deny { code, reason } => {
                assert_eq!(code, "git-dir-protected");
                reason
            }
            Verdict::Allow => panic!("expected a denial"),
        }
    }

    #[test]
    fn it_should_match_a_name_equal_to_dot_git_at_any_depth() {
        for path in [
            "/p/.git",
            "/p/.git/config",
            "/p/a/.git/b",
            "/p/.omhl/.git/HEAD",
        ] {
            assert!(matches(path), "{path}");
        }
    }

    #[test]
    fn it_should_match_a_dot_git_that_only_shows_after_resolving_the_parent_dir() {
        assert!(matches("/p/a/x/../.git/c"));
    }

    #[test]
    fn it_should_not_match_a_name_that_only_resembles_dot_git() {
        for path in [
            "/p/.gitignore",
            "/p/.github/workflows/ci.yml",
            "/p/foo.git",
            "/p/a/git",
        ] {
            assert!(!matches(path), "{path}");
        }
    }

    #[test]
    fn it_should_not_match_a_dot_git_that_the_parent_dir_climbs_out_of() {
        assert!(!matches("/p/.git/../notes.md"));
    }

    #[test]
    fn it_should_not_count_a_dot_git_folder_above_the_root() {
        let path = Path::new("/tmp/.git/proj/notes.md");

        assert!(!has_git_component(path, Path::new("/tmp/.git/proj")));
    }

    #[test]
    fn it_should_count_a_dot_git_folder_below_the_root() {
        let path = Path::new("/tmp/.git/proj/.git/config");

        assert!(has_git_component(path, Path::new("/tmp/.git/proj")));
    }

    #[test]
    fn it_should_count_every_name_when_the_root_is_not_a_prefix_of_the_path() {
        let path = Path::new("/main/docs/../.git/config");

        assert!(has_git_component(path, Path::new("/other")));
    }

    #[test]
    fn it_should_match_when_the_root_still_has_the_parent_dir_the_path_climbs_through() {
        let path = Path::new("/main/docs/../.git/config");

        assert!(has_git_component(path, Path::new("/main/docs/..")));
    }

    #[test]
    fn it_should_build_the_git_dir_protected_denial_on_one_line_asking_the_owner() {
        let verdict = git_dir_protected(Path::new("/p/.git/config"));

        let reason = reason(verdict);

        assert!(
            reason.starts_with("git-dir-protected: /p/.git/config is under"),
            "reason: {reason}"
        );
        assert!(!reason.contains('\n'), "reason: {reason}");
        assert!(
            reason.ends_with("ask the owner and do not change it through any tool"),
            "reason: {reason}"
        );
        assert!(!reason.contains("Bash"), "reason: {reason}");
        assert!(!reason.contains("terminal"), "reason: {reason}");
    }

    #[test]
    fn it_should_cut_a_long_path_of_the_git_dir_protected_denial_at_200_characters() {
        let long = format!("/p/\n{}/.git/config", "Q".repeat(500));

        let reason = reason(git_dir_protected(Path::new(&long)));

        assert_eq!(reason.matches('Q').count(), 196);
        assert!(!reason.contains('\n'), "reason: {reason:?}");
    }
}
