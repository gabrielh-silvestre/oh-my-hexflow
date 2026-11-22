const FOLDER: &str = "wi-w1__login";
const DEFAULT_SPECS: &str = ".omhl/specs";
const CUSTOM_SPECS: &str = "docs/team-specs";
const FROZEN: &str = "spec-frozen";
const IN_WORKTREE: &str = "spec-in-linked-worktree";

mod lock {
    use std::path::Path;

    use crate::workflow::lock::{
        Facts, SpecPath, Tool, Verdict, decide, next_amendment, parse_spec_path,
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
}
