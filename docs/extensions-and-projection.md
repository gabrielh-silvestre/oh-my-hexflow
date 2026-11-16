# Extensions e projeção

Vocabulário em [GLOSSARY.md](../GLOSSARY.md).

## Extensions (arquivo para banco)

Uma extension acrescenta um `check` ou uma `guideline` ao projeto. Ela é escrita à mão, versionada com o repositório e carregada no banco por `omhl load`.

```
.omhl/
  checks/
    lint.toml
    lint.md          # opcional: o texto longo (`how`)
  guidelines/
    commits.toml
```

O id do node vem da pasta e do nome do arquivo: `checks/lint.toml` vira `checks.lint`.

Check:

```toml
blocking = true
how = "Run `cargo clippy --workspace --all-targets -- -D warnings` and fix every warning."
after = ["checks.fmt"]   # opcional

[when]
event = "before-pr"       # fechado: before-commit, before-pr ou before-merge
matcher = "*.rs"         # opcional
```

Guideline:

```toml
kind = "architecture"      # obrigatório e fechado: business ou architecture
text = "Write commit subjects in English, Conventional Commits, at most 80 characters."
```

O `omhl load` recusa a guideline sem `kind` (`kind-missing`) ou com outro valor (`kind-invalid`) e não grava nada; o erro traz a causa e o próximo passo.

O texto longo (`how` no check, `text` na guideline) fica na chave ou num `.md` de mesmo nome ao lado do `.toml`, nunca nos dois. A validação é estrita:
- chave desconhecida, caractere de controle ou texto acima do limite é erro;
- symlink e arquivo que não é regular são recusados;
- a mensagem de erro traz caminho, linha e coluna, nunca o conteúdo do arquivo.

```bash
omhl load --dry-run   # mostra o que mudaria, sem gravar
omhl load             # grava no tenant do .omhl.toml
```

O relatório lista `created`, `revised`, `unchanged`, `skipped` e `orphans`. Carregar duas vezes seguidas não grava nada na segunda.

**Arquivo apagado não aposenta o node.** O id passa a aparecer em `orphans`, e o node continua vigente e injetado na sessão até ser retirado de propósito (`omhl check retire <check-id>`, só para check cujo arquivo sumiu; é definitivo e vale para todos os clones e branches, então só depois de a remoção do arquivo estar mergeada). Um check órfão com `run` não é executado pelo `check run`, que avisa com `check-orphaned`. Assim, trocar de branch não perde nada.

A diretiva órfã sai com `omhl guideline retire <guideline-id>`, com `--by <id-novo>` quando outra a substitui. Vale o mesmo cuidado: é definitivo para todos os clones e branches. Detalhes em [directives-flow.md](directives-flow.md), seção 11.

O hook `SessionStart` nunca grava. Quando as extensions divergem do banco, ele acrescenta ao contexto uma linha `NOTE`, ou uma `WARNING` se houver extension inválida. A linha traz só códigos e contagens e pede `omhl load`. Com check órfão (arquivo apagado, node ainda vigente), ele acrescenta uma segunda linha `NOTE` com os ids dos checks órfãos: a nota manda fazer rebase sobre a `main` (ou restaurar o arquivo) se a branch é anterior ao check, e avisa que `check retire` é definitivo para todos os clones e branches; o `check run` só avisa (veja [mechanical-gates.md](mechanical-gates.md)).

## Projeção (banco para arquivo)

```bash
omhl project
```

Gera um arquivo Markdown por work item em `.omhl/projection/`, com tudo que está vigente para ele:
- decisions;
- premises;
- attestations;
- relations.

Regras da projeção:
- O mesmo estado do banco gera os mesmos bytes.
- Os arquivos são só de leitura e começam com um cabeçalho de "gerado". Um arquivo sem esse cabeçalho naquela pasta é recusado (`not-generated`), nunca sobrescrito.
- A pasta traz o próprio `.gitignore` e não é versionada.

## Interação com `attest` e `close`

`attest` e `close` exigem a árvore git limpa. Uma extension editada e ainda não commitada suja a árvore, então os dois recusam com `snapshot-dirty` até o commit. A projeção não suja a árvore, porque a pasta é ignorada.
