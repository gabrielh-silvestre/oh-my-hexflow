# Rodada 1: revisões independentes do plano do passo 3

Instantâneo revisado: [omhl-v2-core-step3.md](../plans/omhl-v2-core-step3.md) (549 linhas), com o plano mestre [omhl-v2-graph-engine.md](../plans/omhl-v2-graph-engine.md) e o briefing [ralplan-brief.md](ralplan-brief.md). Os dois revisores não viram um ao outro. Estas são as mensagens deles, condensadas sem mudar o conteúdo.

## Architect: sem BLOQUEANTE, 5 IMPORTANTE, 3 MENOR; nenhuma decisão do dono violada

**Antítese.** O plano congela cedo demais um corpo de elo cheio de decisões sem resposta (`Retire`, `from_origin`, `native_catalog`, `external_id`, `alias`, `def_hash`, `requires_state`; §4.3 linhas 234-245; itens 2, 5 e 10 da §2). O portão de congelamento só vem ao fim da T6 (linha 422), mas a T2 já congela o hash dourado (linha 398) e T3 a T5 constroem sobre esse corpo.

**Tensão.** Neutralidade do motor (D1, C7) contra expressividade do Catalog. Schema por registro mais `from`/`to`/`acyclic` não expressa restrições entre registros (transição aberto→fechado do `unit`, D32; "aprovação exige estado").

**Síntese.** Manter o corpo do elo, mas antecipar o portão de congelamento: o dono responde aos itens 5 e 10 antes da T2. Declarar no ADR que invariantes entre registros ficam no adaptador na v1. O `third_graph` deve incluir um caso dessas regras fora do motor.

**Achados.**
1. IMPORTANTE, §4.4 (270-301, 326-337): tipos usados e nunca definidos: `EdgeView`, `RevisionView`, `Page`, `PageRequest`, `TenantInfo`, `TipRow`, `BatchRow`, `Decision`, `NewLink`, `TenantRow`, `ChainBreak`, `VerifyPolicy`. `EdgeView` precisa carregar o id técnico da ponta (para o `base` de um `Retire`) e o `endpoint_state`.
2. IMPORTANTE, §4.2 linha 175 e T1(c): `jsonschema` traz por padrão resolução HTTP/arquivo (reqwest, tokio; não verificado), o que reprova `it_should_depend_only_on_the_allowlisted_crates`. Fixar `default-features = false` e conferir com `cargo tree` no spike da T1.
3. IMPORTANTE, §5 C7 (493): varredura de palavras inteiras em comentários dá falso positivo com `check`, `review`, `unit`, `gate`. Varrer só identificadores e literais de string; manter `git` restrito a `state.rs`.
4. IMPORTANTE, §4.4 e T5 (415): `rusqlite::Connection` é `Send` mas não `Sync`; "Hexlog é Send + Sync" exige `Mutex<Connection>` ou uma conexão por chamada. `BEGIN IMMEDIATE` cobre processos concorrentes.
5. IMPORTANTE, §4.5 ordem de recusa (356-358): validação estática (catálogo, schema, RESERVED_ID) roda antes do replay; depois de uma atualização do plugin que reserve um id, reenvio idempotente de lote antigo falha em vez de devolver `replayed` (contra H:182). Colocar o replay logo após a forma do lote e o tenant.
6. MENOR, §4.3: não está definido o que o `Retire` grava em `def_hash`, nem se aresta com extremo aposentado entra na detecção de ciclo (359). `Retire` copia o `def_hash` da ponta; aresta com extremo aposentado fica fora do ciclo.
7. MENOR, §5: faltam testes de `anchor-mismatch`, de adulteração de `tenants.anchor_doc` e de abrir banco com `user_version` maior que o suportado (um teste de cada, mais um código de erro claro).
8. MENOR, §4.5: "só inteiros" não diz o que fazer com `1.0` ou `1e2`; recusar qualquer literal com ponto ou expoente.

**Pontos de atenção.** D31: o plano reduz "o núcleo conhece o git" a conhecer o formato do SHA (item 1, linha 83); está marcado [DONO], mas é releitura de decisão fechada. D1: `Retire`, `requires_state` e `State` no corpo do elo são vocabulário estrutural, não de domínio; aceitável, o dono deve ver no portão.

## Critic: veredito ITERATE

O plano é sólido; citações H:n conferem; contradições bem levantadas; nenhuma decisão do dono violada. Os problemas estão na verificação.

**Achados maiores.**
1. As travas de arquitetura de C7 se contradizem (§4.2, §5 C7). `it_should_depend_only_on_the_allowlisted_crates` proíbe regex externo, async e rede entre transitivas, mas a lista inclui `jsonschema`, que provavelmente arrasta regex/fancy-regex e reqwest/tokio nas features padrão (não confirmado; o spike (c) da T1 só checa `$ref` externo e rede em tempo de execução). O vocabulário proibido tem `decision`, mas §4.4 define o tipo `Decision` em `store/mod.rs`, que é varrido. `check`, `unit` e `review` são palavras comuns em comentário e a varredura inclui comentários. Não se diz se a comparação ignora caixa nem se `git` é palavra inteira (`digit` falharia). A varredura não cobre `error.rs` nem `lib.rs`. **Correção:** renomear `Decision` para `AppendPlan`; comparação por palavra inteira e sem distinção de caixa; trocar o veto transitivo por lista de crates proibidos (`git2`, `reqwest`, `tokio`, `hyper`), com `jsonschema` em `default-features = false`; ampliar a varredura a `error.rs` e `lib.rs`.
2. Comportamento prometido sem teste nem tipo: `endpoint_state` em `EdgeView` (§2 item 4) e "shadowed" em `NodeView` (item 2) não estão definidos; `CHAIN_BROKEN` na escrita com a política `Tail` não tem teste (a mitigação de R4 depende dele); `where_eq` em `ListFilter` sem teste nem regra de ligação do caminho JSON ao SQL (risco de injeção); `Page`/`PageRequest` indefinidos e o cursor fora do escopo; motivos `anchor-mismatch`, `invalid-body` e `head-not-found` (expected_head inválido) em §4.5 sem teste em §5; o proptest "single byte change in any stored field" diz "qualquer campo" mas §4.5 não diz que `tenants.created_at` e `batches.first_seq`/`item_count` são conferidos. **Correção:** definir os tipos, um teste por comportamento, listar em §4.5 exatamente os campos que `verify_chain` confere, e o proptest sorteia só esses.
3. Regras de `state` ambíguas dentro do corpo hasheado (§4.4, §4.5): não está definido o que acontece quando o `state` vem sem o tipo exigir; se um `Retire` de tipo `requires_state` exige `state`; se a revisão deve reenviar o `state`; se aresta pode ter `state`. Duas leituras gravam corpos diferentes, e o formato congela no portão da T6. **Correção:** tabela de regras (tipo × item × state) com um teste por linha, antes do portão.

**Menores.**
4. T5 junta decide, regras, Facade e cerca de 30 testes sem marco intermediário: dividir em T5a (nó, revisão, `Retire`, lote, idempotência), T5b (aresta, `ForeignNodes`, ciclo), T5c (concorrência).
5. O hash dourado é gerado pela própria implementação Rust e a conferência com o `hashLink` do TS é manual e "fora do CI": gravar na fixture o valor calculado pelo TS e comparar o teste com ele (R1 é o risco que mais pesa para sempre).
6. Nomes de teste com reticências (`..._between_two_native_nodes`, `..._hash`): o briefing pede o nome concreto.
7. Replay depois das checagens estáticas (passo 3 antes do 5): documentar o limite (igual ao achado 5 do Architect).
8. Faltam riscos de skew de versão entre processos (CLI, MCP e hook com `Catalog` ou `ForeignNodes` diferentes) e dizer se `Hexlog` `Send + Sync` usa `Mutex` ou pool.

**Alternativas e riscos.** E1 a E6 são justas. E3-B invalidada com razão (D11). O desvio de E6 (política `Tail`) em relação ao "falha fechado" de H:243 está dito, mas não marcado [DONO]; marcar.

**Para virar APPROVE.** Corrigir os achados 1 a 3 e registrar na fixture o valor dourado calculado pelo TS (menor 5). O resto é polimento.
