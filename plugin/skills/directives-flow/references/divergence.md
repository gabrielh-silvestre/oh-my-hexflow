# A divergência e seus desfechos

Quando consultar: ao gravar uma `divergence` (o agente fugiu da spec congelada) ou ao fechá-la. Os campos de `data` estão na tabela de [append.md](append.md); aqui ficam os valores e o que cada desfecho pede.

| Campo | Valor |
|---|---|
| `source_ref` | O item da spec (`CMP-###` ou `CA-###`) ou o id da diretiva ou premissa |
| `kind` | `missing`, `partial`, `contradicts` ou `unrequested` |
| `text` | O previsto, o feito e o motivo |
| `evidence` | De 1 a 20 itens `{source, summary}` |
| `resolution` | `open`, `spec-stale`, `code-fixed` ou `accepted` |
| `work_item` | O work item da divergência |
| `confirmed_by`, `amendment` | Só em `spec-stale` |

Toda divergência nasce `open`. Os três desfechos são novas revisões da mesma `divergence` (`omhl append` com `base`, o `record_id` de `omhl get divergences.<nome>`):

- `spec-stale`: a spec estava errada e o humano aprovou a emenda. A revisão leva `amendment` (`emenda-NN`, sem `.md`) e `confirmed_by`; o passo a passo está no `SKILL.md`, em "Corrigir a spec depois da aprovação".
- `code-fixed`: a spec estava certa e você corrigiu o código para segui-la.
- `accepted`: a spec estava certa e o humano aceita o desvio. Grave só com a resposta dele; o `accepted` não guarda prova, porque o `confirmed_by` é proibido nele.

Em `code-fixed` e `accepted`, não leve `confirmed_by` nem `amendment`: a guarda recusa com `confirmation-invalid`. Uma divergência que já saiu de `open` não aceita emenda depois. Enquanto fica `open`, ela aparece em toda retro (`omhl report retro`).
