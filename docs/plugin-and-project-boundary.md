# Plugin and project boundary

The plugin brings behavior to every project that installs it. This page says, for each behavior, whether it belongs to the plugin, to a choice the project makes, or to this repository alone. Terms are in [GLOSSARY.md](../GLOSSARY.md).

## Method

What the plugin imposes on every project that uses it: a premise confirmed by a human before anything rests on it, a decision anchored on a guideline or a premise, a spec that is frozen once approved, the history kept in a graph, and the roles of the agents.

A project does not turn a Method behavior off. Changing one is a change of the plugin itself. It lives in the plugin (skills, agents, hooks) and in the `omhl` binary.

## Optional default configuration

What stays off until the project chooses it, and comes with an example the project can copy. The default is a suggestion of the plugin, never an obligation: a project that does not copy the example does not get the behavior.

The example lives in `plugin/examples/` (see [its README](../plugin/examples/README.md)) or as a default inside the binary. The project's own copy lives in its `.omhl/`.

## Exclusive to this repository

What only this repository needs: its checks, its conventions and the language of its code and documents. It lives here, in `.omhl/`, `AGENTS.md` and the documents of the repository, and never in the plugin. A project that installs the plugin must not notice it.

## When the class is unclear

The owner decides. Until the owner chooses a class, the behavior is not planned, built or reviewed as if it had one. The delivery that moves a behavior from one class to another edits the row of that behavior in the table below, in the same delivery.

The directive `.omhl/guidelines/boundary-classification.toml` carries this rule into the planning, the execution and the review.

## Classification

`Today` says how the code differs from that, and says `none` when the behavior is already where its class puts it.

| Behavior | Class | Where it lives | Today |
|---|---|---|---|
| The lock of the spec | Method | The `spec-lock` hook of the plugin and the `omhl` binary | none |
| The role hook of the agents | Method | The `agent-role` hook of the plugin and the matrix in the frontmatter of `plugin/agents/` | none |
| The context injected at session start | Method | The `session-start` hook of the plugin, with the context rendered by the `omhl` binary from the graph | none |
| The three native checks | Optional default configuration | The native catalog of the binary, off until a project turns each one on | none |
| The two native guidelines | Optional default configuration | The native catalog of the binary, off until a project turns each one on | Always on in every marked project: no key turns one off |
| The branch name format | Optional default configuration | A default of the binary that `.omhl/workflow.toml` of the project can change | Fixed in the code as `<type>/wi-<id>__<slug>`, with `main` and `develop` as fixed names: any other name is refused and no key changes it |
| The matrix of requirements by prefix | Optional default configuration | A default of the binary that `[prefix.<name>]` in `.omhl/workflow.toml` of the project only adds to | On in every marked project: without a `workflow.toml`, the binary applies its own matrix |
| The mark of the `develop` branch | Optional default configuration | The command `plugin/commands/omhl-develop-mark.md`, used only by a project that chose the mark | Required by every `before-commit` run on `develop`: no key turns it off |
| The hooks of `wt` | Optional default configuration | The `.config/wt.toml` of the project, copied from an example in `plugin/examples/` | The file exists only in this repository and the plugin has no example of it |
| The git hooks (`omhl hooks install`) | Optional default configuration | The command of the binary, run in each clone of a project that wants the hooks | The command installs a fixed set of git hooks: the project cannot pick one |
| The use of GitHub | Optional default configuration | The pull request step of `plugin/skills/directives-flow/references/cycle.md` and of the skill `review`, replaceable by the project | The cycle always ends in a pull request: no key replaces that step |
| The language of the plugin texts | Method | The skills, agents and references of the plugin, written in English | none |
| The language and the emoji rule of the code | Exclusive to this repository | The directive `.omhl/guidelines/code-in-english-no-emoji.toml` | none |
| The checks and the conventions of this repository | Exclusive to this repository | `.omhl/checks/`, `.omhl/guidelines/`, `AGENTS.md` and `CONTRIBUTING.md` | none |

## Where text lives

Each kind of text has one home, and each fact lives in one home only. The other side links to it and does not copy it.

- `plugin/skills/*/references/`: what the agent consumes while it works, such as the reference of a command or the steps of a protocol.
- `plugin/examples/`: the files a project copies into its `.omhl/`.
- `docs/`: what describes the plugin and the binary to whoever studies them.
- `README.md`: what whoever uses the plugin needs, including the [update notices](../README.md#avisos-de-atualização).
- `CONTRIBUTING.md`: what whoever develops the plugin needs.

The rule covers the text the plugin brings to a project and applies to every work item that follows. An obligation that holds only for this repository is not part of it: it lives in `.omhl/guidelines/` and, described as the class Exclusive to this repository, on this page.
