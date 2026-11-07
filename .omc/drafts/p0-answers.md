# Respostas do dono ao P0 do passo 3

Plano: [omhl-v2-core-step3.md](../plans/omhl-v2-core-step3.md), seção 2 (itens 1, 5, 10, 11, 16, 19) e P7 da seção 3. Uma resposta por item, na ordem em que o dono as dá. Ainda não aplicadas ao plano.

- **Item 1 (snapshot no núcleo): aceito.** O motor confere só o formato do snapshot (40 ou 64 hex, SHA-1 ou SHA-256; confirmado pelo dono depois, o "40 hex" da pergunta era abreviação) e confia no `omhl` para ler o git e checar a árvore limpa (`SnapshotProbe`, passos 4 e 5). O motor não prova que o SHA existe. `git2` e `gix` seguem proibidos.
- **Item 5 ("antes de um nativo"): aceito.** A relation com origem nativa é um record do projeto (D13), não altera o nativo; vale com pelo menos um extremo no tenant. Exemplo: `checks.run-tests after checks.lint-docs`.
- **Item 10 (Retirement no passo 3): incluir.** A variante `Retirement` (revision sem dado, `base` obrigatória) entra já no passo 3, antes do congelamento do formato.
- **Item 11 (regras do `snapshot` no corpo): tabela da seção 4.5 confirmada.** Obrigatório e não herdado nos nodes de tipo `requires_snapshot` e em cada revision deles; proibido em work item, relation e Retirement; `external_id` também não herdado (linha X4). Um teste por linha.
- **Item 16 (`@alias`): cortar por completo** (escolha do dono, contra a recomendação de manter só em `base`). Sai o `alias` do item do lote e o `batch.aliases` do corpo do record; `base` passa a referir só record id já gravado. Consequência a refletir no plano: revisar um node criado no mesmo lote exige um segundo lote; contagens de teste, tabela de mutação e CHECKED_FIELDS mudam e precisam de nova conferência.
- **Item 19 (vocabulário estrutural no corpo): aceito como estrutura do motor.** `Retirement`, `requires_snapshot`, `Snapshot`, `from_origin`/`to_origin`, `native_catalog` e `def_hash` ficam listados no princípio 1 (P1) e fora do veto da trava de vocabulário.
- **P7 (forma do campo de snapshot): confirmada.** `{kind, id}` com `kind` enum fechado em Rust, serializado como `"git-commit"`; critérios (i) a (iv) para aceitar um tipo novo ficam como regra escrita no plano.

Resumo: os 7 itens do P0 estão respondidos. Única resposta que muda o plano além de marcar "decidido": o corte do `alias` (item 16).
