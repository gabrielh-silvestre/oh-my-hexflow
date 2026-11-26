# omhl project

## Who uses it, at which stage and with which filter

| Role | Stage | Command |
|---|---|---|
| session | plan-consensus 6, plan-consensus 7, plan-consensus 8, review 4, review 7 | `omhl project` |

## Quando usar

Para regenerar a projeção do projeto: um arquivo Markdown por work item em `.omhl/projection/`, com tudo o que está vigente para ele (decisões, premissas, attestations, relations, e as seções de gaps e de divergências). Use quando precisar ler o estado de um work item como arquivo, ou depois de gravar registros e querer a projeção em dia. Não grava nenhum registro no banco; só escreve arquivos, e só dentro de `.omhl/projection/`.

## Entradas

- Nenhum argumento.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco, fora da pasta do projeto; o `--project-dir` precisa existir.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. O padrão é `agent`; `human` mostra os mesmos fatos em blocos; `json` é a saída para máquinas.

## Saída

Texto no stdout, saída 0: `✔ project · <dir> · N written · N unchanged · N removed`, e uma linha `- written · <arquivos>` e outra `- removed · <arquivos>` só quando a lista não é vazia. Os nomes dos `unchanged` só são contados no texto. Com `--format json` é uma linha JSON: `dir` (a pasta da projeção, `.omhl/projection`) e três listas de nomes de arquivo: `written` (criados ou alterados), `unchanged` (já estavam iguais) e `removed` (de work items que não existem mais). O mesmo estado do banco gera os mesmos bytes, então rodar duas vezes seguidas devolve tudo em `unchanged`.

Os arquivos são só de leitura e começam com o cabeçalho `omhl: generated file, do not edit`. A pasta traz o próprio `.gitignore` e não é versionada, então a projeção não suja a árvore do git. Não edite esses arquivos.

## Erros

Um erro sai no stderr, com saída 1, em texto (`✘ código · mensagem`; com `--format json`, numa linha JSON). Uma opção desconhecida é erro de uso do clap, com saída 2 e sem código. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `projection-failed` (saída 1): a projeção não pôde ser escrita. O detalhe traz o caminho e o código da causa: `not-generated` (já existe na pasta um arquivo sem o cabeçalho de gerado, que o comando nunca sobrescreve: mova-o) ou `too-many-work-items` (há work items demais para a projeção).
- `no-project-root` (saída 1): não há raiz de projeto para escrever a pasta. Rode dentro do projeto.
- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `db-inside-project` (saída 1): o `--db` aponta para dentro do projeto. Use um caminho fora dele.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Gera a projeção e repete: na segunda vez nada muda:

```omhl-example fixture=project
$ omhl project
✔ project · .omhl/projection · 1 written · 0 unchanged · 0 removed
- written · work-items.w1.md
$ omhl project
✔ project · .omhl/projection · 0 written · 1 unchanged · 0 removed
```

Um work item novo ganha o próprio arquivo:

```omhl-example fixture=project
$ omhl project
✔ project · .omhl/projection · 1 written · 0 unchanged · 0 removed
- written · work-items.w1.md
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"work-items.w2","type":"work-item","data":{"title":"Second item","status":"open"}}}]}
EOF
✔ append · 1 record
- <record-id> · seq 3
$ omhl project
✔ project · .omhl/projection · 1 written · 1 unchanged · 0 removed
- written · work-items.w2.md
$ omhl project --format json
{"dir":".omhl/projection","removed":[],"unchanged":["work-items.w1.md","work-items.w2.md"],"written":[]}
```
