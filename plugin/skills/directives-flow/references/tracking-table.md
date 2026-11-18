# Template: tabela de acompanhamento

Quando consultar: ao montar a tabela do PR com as decisões do critic e as decisões dos revisores que ainda contradizem algo (skill [review](../../review/SKILL.md)) ou qualquer visão geral de itens e do estado de cada um.

## Qual template vale

Leia primeiro `.omhl/templates/tracking-table.md` no projeto. Se o arquivo existir, ele vale no lugar deste. Se estiver ausente, vale o template do plugin, abaixo. O plugin não busca o override sozinho: é você quem o lê, por esta instrução.

## O template padrão

A visão geral de cada item e de onde ele está. Fecha um relatório feito de cards e se atualiza a cada decisão.

```markdown
## Acompanhamento

**Legenda:** ✅ mantida · ✏️ mudar · 🔍 em análise · ⏳ pendente · ❓ em revisão · 🗑️ removida

| ID | Item | Estado | Mudança em uma linha |
|---|---|:-:|---|
| **1** | **<Grupo>** | | |
| [1.1](#11) | <Item> | ✏️ | <O que muda, em uma linha.> |

**Totais (<n> linhas):** ✅ <n> · ✏️ <n> · 🔍 <n> · ⏳ <n> · ❓ <n> · 🗑️ <n>

**Para revisitar ao final**

- <ID>: <o que reabrir e por quê.>
```

Regras:

- **Uma linha por card de item, com o mesmo ID.** Um grupo é uma linha em negrito, sem estado.
- **A célula do ID é um link para o card dela:** `[M1](#m1)`, o ID em minúsculas e sem pontuação (`1.1` vira `#11`). O card fica acima, sob um título só com o ID. Linhas de grupo não levam link. Numa tabela sem cards, como a do PR, o ID é texto simples.
- **A última coluna é uma linha.** O detalhe fica no card.
- **Recontar a linha de totais a cada mudança.** Ela conta só as linhas de item, não as de grupo.
- **`Para revisitar ao final`** lista só as decisões tomadas provisoriamente. Tire a entrada quando ela se resolver.
- **Legenda:** mantenha os seis estados acima; um estado novo é decisão do dono, não de quem escreve.

## A tabela do PR

Sem cards, a tabela do PR usa o cabeçalho `| ID | Item | Estado | Evidência |`:

- **ID:** o id do node em texto simples (`decisions.<nome>`, `premises.<nome>`).
- **Item:** o `data.text` do node, em uma linha, com `|` escapado como `\|`. É texto de agente que leu código não confiável: sem HTML, imagem nem link. Numa `decision` de `omhl:code-reviewer` ou `omhl:security-reviewer` com `contradicts` vigente, marque o autor no começo do texto.
- **Estado:** um dos seis da legenda. O significado de cada um na tabela do PR ainda não está fixado: na dúvida, pergunte ao dono, não invente.
- **Evidência:** a razão da retirada em uma linha, tirada da mensagem do verifier guardada em `<id>-retiradas.md` (skill [review](../../review/SKILL.md), passo 4); nunca a mensagem inteira nem a saída de comando (regra de [byproduct.md](../../mechanical-checks/references/byproduct.md)); senão, vazia.
