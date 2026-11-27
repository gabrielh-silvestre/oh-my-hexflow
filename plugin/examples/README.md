# Examples

Copyable files for the `.omhl/` folder of a project. Every example is to copy and adjust. Every example is off until someone copies it: nothing here runs, and nothing here is read by `omhl`, until the file is inside the `.omhl/` of a marked project.

The layout mirrors `.omhl/`:

| In this folder | Copy it to |
|---|---|
| `checks/` | `.omhl/checks/` of the project |
| `guidelines/` | `.omhl/guidelines/` of the project |
| `workflow.toml` | `.omhl/workflow.toml` of the project |
| `workflows/interview-native-run.toml` | `.omhl/workflow.toml` of the project |

Each folder is copied over the folder of the same name in `.omhl/`: keep the `.toml` and its `.md` sibling together, and copy only the files the project wants. A file that already exists in the project is yours to merge by hand.

Every `.toml` is a file the loader accepts today. After copying, check the result without writing anything:

```bash
omhl load --dry-run
```

It reports every problem and warning by file. When it is clean, run `omhl load`.

## What each example shows

- `checks/unit-tests`: a mechanical check, with the command in `run`, a time limit, the rule that says what passing means and the event that triggers it.
- `checks/format-sources` and `checks/lint-sources`: two mechanical checks in order, the second one with `after` naming the first.
- `checks/release-notes`: a text check, with no `run`: the instruction is in the `.md` file next to the `.toml`, and the session answers it.
- `guidelines/plain-language`: a `business` guideline with the text inside the `.toml`.
- `guidelines/layered-dependencies`: an `architecture` guideline with the text in the `.md` file next to the `.toml`.
- `workflow.toml`: the settings a project can change, such as the size of a spec, the threshold of promotion and what a branch prefix requires.
- `checks/rust-fmt.toml`, `checks/rust-clippy.toml` and `checks/rust-test.toml`: the Rust format, lint and test checks, each unsetting the Rust flag variables before `cargo`.
- `checks/node-test.toml`: a test check that runs `npm test` before push.
- `checks/python-test.toml`: a test check that runs `pytest` before push.
- `workflows/interview-native-run.toml`: a full `workflow.toml` with the interview threshold and weights, and a native check turned on with its own command.
