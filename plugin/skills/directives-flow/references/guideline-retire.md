# omhl guideline retire

## Quando usar

Para retirar uma diretiva (`guideline`) cujo arquivo `.omhl/guidelines/<nome>.toml` foi apagado da árvore de trabalho. Ela sai do contexto da sessão e o próximo `omhl load` não a recria. A retirada é definitiva para todos os clones e branches, porque o banco é do usuário e é append-only: retire só depois de a remoção do arquivo estar mergeada, e peça ao humano antes. Numa branch anterior à diretiva, faça rebase em vez de retirar.

## Entradas

- `<ID>`: o id da diretiva, como `guidelines.brief`. Precisa ser uma diretiva vigente do repositório, não nativa, com o arquivo já apagado.
- `--by <id>`: a diretiva que substitui a retirada. Opcional; com ela o comando grava no mesmo pedido a relation `supersedes` (da nova para a antiga) e a retirada. A sucessora precisa ser vigente e diferente da retirada.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. O `--project-dir` precisa existir.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. O padrão é `agent`; `human` mostra os mesmos fatos em blocos; `json` é a saída para máquinas.

## Saída

Texto no stdout, saída 0: `✔ guideline retire · retired <id>`, com `· superseded by <id>` quando há `--by`. Com `--format json` é uma linha JSON: `retired` (o id retirado) e `superseded_by` (o id de `--by`, ou `null`). Decisões que só se apoiavam nessa diretiva passam a reprovar `decisions-anchored` até ganharem outro apoio.

## Erros

Um erro sai no stderr, com saída 1, em texto (`✘ código · mensagem`; com `--format json`, numa linha JSON). Cada recusa traz a causa em `message` e o próximo passo em `next_step` (no texto, a linha `Next:`), sem repetir o valor de entrada. A falta do `<ID>` é erro de uso do clap, com saída 2 e sem código. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `guideline-not-found` (saída 1): o id não é uma diretiva vigente do repositório (não existe, já foi retirada, é nativa ou é de outro tipo). Confira o id.
- `guideline-not-orphaned` (saída 1): o arquivo da diretiva ainda existe na árvore. Apague o arquivo, mergeie a remoção e repita.
- `successor-not-current` (saída 1): o id de `--by` não é uma diretiva vigente. Carregue a sucessora com `omhl load` antes, ou retire sem `--by`.
- `successor-is-self` (saída 1): o `--by` é a própria diretiva retirada. Passe outra, ou retire sem `--by`.
- `supersedes-cycle` (saída 1): a sucessora já é superada pela retirada, direto ou por outras. Escolha outra sucessora.
- `guideline-retire-by-command` (saída 1): a retirada veio pelo `omhl append`. Só este comando a grava.
- `INVALID_INPUT` (saída 1): um id tem forma inválida (`semantic-id-format`). O `path` do detalhe diz qual argumento.
- `invalid-session-id` (saída 1): `CLAUDE_CODE_SESSION_ID` está definida mas vazia, não é UTF-8, passa de 100 unidades UTF-16 ou é igual ao marcador `outside-claude-code`; nada é gravado e o erro não mostra o valor (`path` vazio). Remova a variável ou rode o `omhl` numa sessão do Claude Code.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Retira a diretiva órfã. Repetir a retirada falha, porque ela já não é vigente:

```omhl-example fixture=retire-guideline
$ omhl guideline retire guidelines.brief
✔ guideline retire · retired guidelines.brief
$ omhl guideline retire guidelines.brief
2> ✘ guideline-not-found · the id is not a current guideline stored in the repository
2>   - /id
2> Next: pass the id of a current guideline that is not native.
# exit 1
```

Com `--format json` a sucessora aparece como `null` quando não há `--by`:

```omhl-example fixture=retire-guideline
$ omhl guideline retire guidelines.brief --format json
{"retired":"guidelines.brief","superseded_by":null}
```

Recusa: a sucessora é a própria diretiva:

```omhl-example fixture=retire-guideline
$ omhl guideline retire guidelines.brief --by guidelines.brief
2> ✘ successor-is-self · a guideline cannot be its own successor
2>   - /by
2> Next: pass a different guideline in --by, or retire without --by.
# exit 1
```

Recusa: a sucessora não existe:

```omhl-example fixture=retire-guideline
$ omhl guideline retire guidelines.brief --by guidelines.new
2> ✘ successor-not-current · the successor is not a current guideline
2>   - /by
2> Next: pass the id of a current guideline in --by, or retire without --by.
# exit 1
```

Recusa: a diretiva não existe:

```omhl-example fixture=retire-guideline
$ omhl guideline retire guidelines.nope
2> ✘ guideline-not-found · the id is not a current guideline stored in the repository
2>   - /id
2> Next: pass the id of a current guideline that is not native.
# exit 1
```
