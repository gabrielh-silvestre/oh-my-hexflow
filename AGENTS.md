# AGENTS.md

- Checks mecânicos devem estar sempre alinhados com o banco: evite alterar checks durante o desenvolvimento; se alterar, rode `omhl load` antes de qualquer `check run`.
- O `omhl check run` recusa com `spec-stale` (em qualquer evento) se o TOML e o banco divergem. Detalhes em [docs/mechanical-gates.md](docs/mechanical-gates.md).
