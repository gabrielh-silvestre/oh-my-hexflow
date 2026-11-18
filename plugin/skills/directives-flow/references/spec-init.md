# omhl spec init

## Quando usar

Para criar a pasta da spec de um work item `feat/` ou `fix/`, com a primeira parte (`parte-01.md`) já com as cinco seções vazias. Rode antes de escrever a spec e antes de `omhl gate-worktree`: o gate desses prefixos exige uma spec válida. A pasta nasce no diretório principal do repositório, mesmo que você esteja numa worktree.

## Entradas

- `<WORK_ITEM>`: o id do work item, como `work-items.w1`. Precisa existir e estar aberto.
- `<SLUG>`: o nome da pasta depois do id, de 1 a 64 caracteres de `a-z`, `0-9` e `-`, sem começar por `-`. A pasta será `wi-<id>__<slug>`.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. O `--project-dir` precisa existir.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. O padrão é `agent`; `human` mostra os mesmos fatos em blocos; `json` é a saída para máquinas.
- Precisa de um repositório git (o comando acha o diretório principal por ele).

## Saída

Texto no stdout, saída 0: `✔ spec init · <work item> · <slug>`, as linhas `dir:` (a pasta criada) e `path:` (o `parte-01.md`) e o `Next:` com o passo seguinte (preencher as cinco seções e rodar `omhl spec validate`). Com `--format json` é uma linha JSON: `dir`, `path`, `slug` e `work_item`. O comando também escreve um `.gitignore` com `*` na pasta de specs: a spec não vai para o git.

## Erros

Um erro sai no stderr, com saída 1, em texto (`✘ código · mensagem`; com `--format json`, numa linha JSON). A falta de um argumento é erro de uso do clap, com saída 2 e sem código. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `spec-slug-invalid` (saída 1): o slug não segue a regra. O ponteiro é `/slug`. Corrija o nome.
- `spec-exists` (saída 1): o work item já tem uma pasta de spec, de qualquer slug. Edite a existente; se a spec já foi aprovada, a correção é uma emenda.
- `work-item-not-found` (saída 1): o work item não existe. Grave-o com `omhl append` antes.
- `work-item-closed` (saída 1): o work item já foi fechado. Use um work item aberto.
- `spec-store-unusable` (saída 1): a pasta de specs não pode ser lida ou escrita, passou de 10 000 entradas, ou o work item tem mais de uma pasta `wi-<id>__*`. Corrija a pasta no diretório principal.
- `probe-failed` (saída 1): o git não respondeu (fora de um repositório). Rode dentro de um checkout.
- `invalid-workflow-config` (saída 1): o `.omhl/workflow.toml` é inválido. A mensagem traz o arquivo e a posição; corrija-o.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Cria a spec do work item `w1` (`<repo>` é a raiz do repositório):

```omhl-example fixture=git
$ omhl spec init work-items.w1 export-reports
✔ spec init · work-items.w1 · export-reports
  dir: <repo>/.omhl/specs/wi-w1__export-reports
  path: <repo>/.omhl/specs/wi-w1__export-reports/parte-01.md
Next: fill in the five sections of the part file, then run omhl spec validate work-items.w1.
```

Com `--format json` o resultado sai para máquinas:

```omhl-example fixture=git
$ omhl spec init work-items.w1 export-reports --format json
{"dir":"<repo>/.omhl/specs/wi-w1__export-reports","path":"<repo>/.omhl/specs/wi-w1__export-reports/parte-01.md","slug":"export-reports","work_item":"work-items.w1"}
```

Recusa: o work item já tem spec, mesmo com outro slug:

```omhl-example fixture=git
$ omhl spec init work-items.w1 export-reports
✔ spec init · work-items.w1 · export-reports
  dir: <repo>/.omhl/specs/wi-w1__export-reports
  path: <repo>/.omhl/specs/wi-w1__export-reports/parte-01.md
Next: fill in the five sections of the part file, then run omhl spec validate work-items.w1.
$ omhl spec init work-items.w1 other
2> ✘ spec-exists · a spec folder of this work item already exists
2>   - /work_item
# exit 1
```

Recusa: slug fora da regra:

```omhl-example fixture=git
$ omhl spec init work-items.w1 Bad_Slug
2> ✘ spec-slug-invalid · the slug must have 1 to 64 characters of a-z, 0-9 and -, and not start with -
2>   - /slug
# exit 1
```

Recusa: work item que não existe:

```omhl-example fixture=git
$ omhl spec init work-items.zz x
2> ✘ work-item-not-found · work-item-not-found at /work_item
2>   - /work_item · work-item-not-found
# exit 1
```
