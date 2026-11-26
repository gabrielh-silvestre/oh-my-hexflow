# Consulta ao banco

Índice das páginas dos comandos que leem o banco do projeto. As regras que valem o tempo todo estão na seção "Consulta" do [SKILL.md](../SKILL.md); aqui ficam os detalhes que só uma página resolve.

- Um `get` de id que não existe não é erro: diz `node not found` (com `--format json`, `{"node":null}`) e sai 0. Um id mal formado é erro. Detalhes em [get.md](get.md).
- O `--id-prefix` do `list` casa por segmento inteiro e começa por um namespace de tipo (`premises`, `work-items`, `checks`). Detalhes em [list.md](list.md).
- O `--db` com caminho novo cria o banco, fora da pasta do projeto; o `--project-dir` precisa existir. Vale para todos os comandos abaixo.

| Comando | Página |
|---|---|
| Achar nós por tipo e filtro | [list.md](list.md) |
| Ler um nó pelo id | [get.md](get.md) |
| Ler o contexto de sessão | [render.md](render.md) |
| Verificar a cadeia de hashes | [verify.md](verify.md) |
| Regenerar a projeção | [project.md](project.md) |
| Ler o relatório dos gaps | [report-answers.md](report-answers.md) |
| Ler o relatório das premissas repetidas | [report-recurrences.md](report-recurrences.md) |
| Listar as sessões que gravaram (`omhl report sessions`) | [report-sessions.md](report-sessions.md) |
| Erros comuns | [errors.md](errors.md) |
