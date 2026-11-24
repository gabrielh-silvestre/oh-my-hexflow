Text check, not run by omhl: you execute it. Before opening the PR, call the `oh-my-hexflow:code-reviewer` agent on the documentation and comments the PR adds or changes (`README.md`, `GLOSSARY.md`, `AGENTS.md`, `docs/*.md`, and `///`/`//!` in `crates/*/src`; not `plugin/`, `docs/adr/` or `.omhl/`), and fix every finding before the PR. A check without `run` receives the outcome of the `omhl attest` the human records after the findings are fixed. The mechanical sweep (`cargo test -p omhl --lib doc_surface::`) already blocks numbers without backing, counts, copied lists and doc blocks over the limit; this check covers what needs interpretation.

Ask the reviewer to flag text that:
- restates what the code already says with no number or list (a comment that paraphrases the next line, a doc that narrates the body, a markdown paragraph that describes a function);
- would become false if only the code changed and that no test checks;
- documents a fact instead of the reason, the contract the reader must follow, or what a test guards.

The fix is never an exception marker: improve the code so the text is no longer needed, or delete it. What stays is the reason, the instruction to the reader and what has a guard. The reviewer reports file:line and the finding, and does not edit.
