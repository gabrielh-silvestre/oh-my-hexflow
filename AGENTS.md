# AGENTS.md

- Checks mecânicos devem estar sempre alinhados com o banco: evite alterar checks durante o desenvolvimento; se alterar, rode `omhl load` antes de qualquer `check run`.
- O `omhl check run` recusa com `spec-stale` (em qualquer evento) se o TOML e o banco divergem ou ficou inválido. Se o TOML foi apagado, o check é órfão: não roda e o `check run` avisa (`check-orphaned`) sem bloquear. Detalhes em [docs/mechanical-gates.md](docs/mechanical-gates.md).
- Fluxo por diretivas e premissas: grave o work item e as premissas pela CLI (`omhl append`; a attestation, por `omhl attest`), peça a confirmação do humano e copie pergunta e resposta literais em `confirmed_by`; só então crie a worktree com `wt switch -c`.
- A spec fica em `.omhl/specs/`. Quando a branch do work item existe, o hook `omhl hook spec-lock`, ativo no plugin, a congela: nunca edite uma spec aprovada. Para corrigi-la, registre uma `divergence` e crie `emenda-NN.md`. Detalhes em [docs/directives-flow.md](docs/directives-flow.md).
