# AGENTS.md

- Checks mecânicos devem estar sempre alinhados com o banco: evite alterar checks durante o desenvolvimento; se alterar, rode `omhl load` antes de qualquer `check run`. Detalhes em [docs/mechanical-gates.md](docs/mechanical-gates.md).
- Na máquina do dono, o gate local roda `cargo test --workspace --lib --locked`. Ao mexer em área coberta por `tests/`, rode só o módulo afetado (integração: `cargo test -p <crate> --test integration <modulo>::`; e2e, só no omhl: `--test e2e <modulo>::`; unitário: `--lib <modulo>::`); `cargo test` sem filtro roda a suíte inteira.
- Fluxo por diretivas e premissas: grave o work item e as premissas pela CLI (`omhl append`; a attestation, por `omhl attest`), peça a confirmação do humano e copie pergunta e resposta literais em `confirmed_by`; só então crie a worktree com `wt switch -c`.
- Nunca edite uma spec aprovada. Para corrigi-la, registre uma `divergence` e crie `emenda-NN.md`. Detalhes em [docs/directives-flow.md](docs/directives-flow.md).
