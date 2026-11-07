# Cadeia de hash e verify_chain: pesquisa de reuso

Consulta feita em 2026-11-07, para as caixas cinza `hexlog` (nível 0), `engine` e `engine/verify` do crate `hexlog` (motor síncrono sobre SQLite, `rusqlite` bundled). A lógica procurada é a cadeia de hash por tenant (`hash = sha256hex(prev_hash + JCS(corpo))`, `seq` contíguo, âncora por tenant, lotes atômicos) e o `verify_chain` (recalcula hashes, confere colunas derivadas contra o corpo, devolve lista ordenada de quebras e detecta cauda truncada com `expected_chain_head`). O formato do hash é decisão tomada: porta do Hexlog TS com JCS (RFC 8785).

Resultado: nenhum candidato é apto. Todos os crates Rust ficam abaixo de 1000 estrelas, e os três projetos acima disso (Trillian, Rekor, immudb) são servidores Go com rede. A cadeia e o verify viram gap interno.

## Tabela de Conteúdos

- [Critérios de aptidão](#critérios-de-aptidão)
- [vigil-audit](#vigil-audit)
- [attest-ledger-core](#attest-ledger-core)
- [Crates menores de auditoria](#crates-menores-de-auditoria)
- [Árvores Merkle em Rust](#árvores-merkle-em-rust)
- [Canonicalização JCS](#canonicalização-jcs)
- [Servidores de log transparente e bancos imutáveis](#servidores-de-log-transparente-e-bancos-imutáveis)
- [Descartados por licença](#descartados-por-licença)
- [Gap interno e fundamentos](#gap-interno-e-fundamentos)
- [Links Úteis](#links-úteis)

---

## Critérios de aptidão

### O que é?

O filtro que o dono definiu para decidir se uma biblioteca substitui código interno. Os critérios técnicos vieram no pedido original; o corte de popularidade e saúde veio depois.

### O que cobre?

| critério | regra |
|---|---|
| manutenção | última release e último commit; mais de 18 meses sem release conta como ruim, salvo crate estável e terminado |
| adoção | downloads recentes no crates.io; **no mínimo 1000 estrelas no GitHub** |
| saúde | documentação, testes e badges conferidos no repositório; atualizações recentes |
| licença | compatível com MIT ou Apache-2.0 |
| toolchain | compila em Rust 1.97, edition 2024 |
| API | síncrona; sem obrigar `Rc`, `Arc`, `RefCell` ou `'static` |
| dependências | fecho normal sem `git2`, `gix`, `reqwest`, `ureq`, `hyper`, `tokio`, `async-std` |

### Veredito

Medi a compilação com `cargo check` num projeto descartável (rustc 1.97.0, cargo 1.97.0) contendo `attest-ledger-core`, `vigil-audit`, `serde_jcs`, `serde_json_canonicalizer`, `ct-merkle` e `rs_merkle`. Os seis compilam em edition 2024 e nenhum traz crate proibido no fecho normal. Pacotes únicos no fecho (`cargo tree -e normal`, contando o próprio crate): `attest-ledger-core` 52, `vigil-audit` 59, `serde_jcs` 5, `serde_json_canonicalizer` 19, `ct-merkle` 8, `rs_merkle` 10. A API REST do GitHub sem token estourou o limite de taxa no meio da coleta; por isso testes e CI de vários repositórios ficaram `n/v` (não verificado).

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [vigil-audit](https://crates.io/crates/vigil-audit)

### O que é?

Ledger de auditoria do projeto Vigil (guarda de agentes de IA): cadeia de hash e `verify_chain` sobre `rusqlite`, com JCS. É a peça mais próxima do que a caixa precisa.

### O que cobre?

| item | valor |
|---|---|
| última release | 0.5.0, 2026-07-08 |
| último commit | 2026-09-17 (repo `duncatzat/vigils`) |
| downloads recentes | 169 |
| estrelas | 74 |
| mantenedores | 4 contribuidores; 2 concentram 120 de 126 commits |
| licença | Apache-2.0 |
| saúde | docs.rs ok; 87 arquivos em `docs/`; cerca de 100 arquivos de teste; 8 workflows de CI; 4 badges |
| bloqueios | hash próprio; acoplamento a `vigil-types` e `vigil-redaction`; `rusqlite` 0.33 pode conflitar com a versão do projeto |

O hash do Vigil é `sha256(domain_tag ‖ u32_be(32) ‖ prev_hash ‖ u64_be(len) ‖ payload_JCS ‖ u32_be(8) ‖ created_at_i64_be)`. A versão 2 também amarra `session_id`, `event_type` e `redacted_text`. Não há tenant, `seq` contíguo nem âncora, e a fórmula não é a do Hexlog.

### Veredito

**Não apto** (74 estrelas; formato incompatível). Vale como referência de desenho: o `chain_version` por evento e a decisão de amarrar colunas derivadas ao hash (ADR 0002 e o comentário de `hash.rs`, motivo: `VIGIL-SEC-001`) tocam o mesmo problema da conferência de colunas derivadas no `verify_chain`. O próprio código admite que a cadeia sem âncora externa não impede a reescrita total.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [attest-ledger-core](https://crates.io/crates/attest-ledger-core)

### O que é?

Núcleo puro (sem persistência) de um ledger com cadeia de registros, âncora assinada com Ed25519 e verificador independente. O repositório é `statecrafting/attest-ledger`.

### O que cobre?

| item | valor |
|---|---|
| última release | 0.2.0, 2026-10-10 |
| último commit | 2026-10-28 |
| downloads recentes | 10.133 |
| estrelas | 0 |
| mantenedores | 1 dono no crates.io; criado em 2026-07-14 |
| licença | Apache-2.0 |
| saúde | docs.rs ok; README de 7,5 KB com exemplo; sem badges; testes e CI `n/v` |
| bloqueios | hash sobre o envelope `LedgerRecord`, canonicalizado por `canonical-keysort-json` (não é JCS); exige `ed25519-dalek`; ~3 meses de vida |

Cobre o que mais se parece com a nossa caixa: `verify_chain_with_head` prova completude da cauda contra um `HeadCommitmentV1` obtido por canal confiável, a mesma ideia de `expected_chain_head`. O README afirma que um head guardado junto com o ledger não dá garantia de frescor, que é a mesma limitação que o `expected_chain_head` tem.

### Veredito

**Não apto** (0 estrelas). Referência de desenho para o modelo de head commitment e para a separação entre quem produz e quem verifica.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Crates menores de auditoria

### O que é?

Quatro crates de log de auditoria com hash chain, todos criados em 2026 e com 0 a 4 estrelas.

### O que cobre?

| crate | versão e data | downloads recentes | estrelas | licença | bloqueios |
|---|---|---|---|---|---|
| [tamper-audit](https://crates.io/crates/tamper-audit) | 0.2.3, 2026-10-24 | 2.051 | 1 | MIT OR Apache-2.0 | `async-trait`, `sqlx` opcional (não é `rusqlite`); formato próprio; 5 issues abertas; 3 badges |
| [audit-trail](https://crates.io/crates/audit-trail) | 1.2.0, 2026-11-06 | 517 | 0 | Apache-2.0 OR MIT | log em arquivo, não SQLite; formato próprio; 6 badges |
| [openmoat-audit](https://crates.io/crates/openmoat-audit) | 0.16.0, 2026-11-06 | 1.532 | 4 | MIT OR Apache-2.0 | `rust_version` 1.99 acima de 1.97, não compila aqui; depende de `openmoat-core`; 39 versões em um mês; 20 issues abertas |
| [smooai-audit](https://crates.io/crates/smooai-audit) | 0.4.1, 2026-08-25 | 4.067 | 0 | MIT | SDK cliente de serviço; `reqwest`/`tokio` opcionais |

Testes e CI destes quatro ficaram `n/v`.

### Veredito

**Não aptos** (todos abaixo de 1000 estrelas; nenhum reproduz o formato do Hexlog).

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Árvores Merkle em Rust

### O que é?

Bibliotecas de árvore Merkle com provas de inclusão e consistência. Uma árvore Merkle não é a cadeia linear `prev_hash -> hash` do Hexlog; entram na pesquisa porque seriam a base de um log verificável mais forte.

### O que cobre?

| crate | versão e data | downloads recentes | estrelas | licença | observações |
|---|---|---|---|---|---|
| [rs_merkle](https://crates.io/crates/rs_merkle) | 1.5.0, 2025-02-24 | 573.894 | 240 | MIT/Apache-2.0 | último commit 2025-02-24 (cerca de 20 meses: crate estável, mas acima do limite de 18 meses); 15 issues abertas; 3 badges; docs.rs ok |
| [ct-merkle](https://crates.io/crates/ct-merkle) | 0.3.0, 2026-05-23 | 162.076 | 41 | MIT/Apache-2.0 (GitHub mostra NOASSERTION) | Merkle no estilo RFC 6962; 1 mantenedor; 3 badges; docs.rs ok |

### Veredito

**Não aptos** (240 e 41 estrelas; não cobrem a cadeia linear). O `ct-merkle` fica de olho para o dia em que o Hexlog precisar de provas de consistência.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Canonicalização JCS

### O que é?

A peça de [RFC 8785](https://www.rfc-editor.org/rfc/rfc8785) (JSON Canonicalization Scheme) que a fórmula do hash exige. É a única parte da cadeia que existe pronta, em duas versões.

### O que cobre?

| crate | versão e data | downloads recentes | estrelas | licença | observações |
|---|---|---|---|---|---|
| [serde_jcs](https://crates.io/crates/serde_jcs) | 0.2.0, 2026-03-25 | 4.011.297 | 8 | MIT OR Apache-2.0 | 5 pacotes no fecho; 1 issue aberta; README de 1,5 KB, sem badges; docs.rs ok; é o crate que o `vigil-audit` usa |
| [serde_json_canonicalizer](https://crates.io/crates/serde_json_canonicalizer) | 0.4.1, 2026-10-13 | 12.326.225 | 23 | MIT | 19 pacotes no fecho; 2 issues abertas; 3 badges; docs.rs ok |

A conformidade numérica (formatação de números, escapes) dos dois **não foi verificada**.

### Veredito

**Não aptos** pelo critério de 1000 estrelas. Os downloads mostram uso real, mas o corte do dono manda. Para decidir entre os dois (ou escrever o canonicalizador interno), rode os vetores do RFC 8785 e os vetores da porta TS contra cada um antes de fechar.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Servidores de log transparente e bancos imutáveis

### O que é?

Projetos maduros de log à prova de adulteração, todos servidores Go com rede e árvore Merkle global, sem cadeia por tenant e sem encaixe embutido em SQLite.

### O que cobre?

| projeto | release | estrelas | licença | saúde | motivo do descarte |
|---|---|---|---|---|---|
| [Trillian](https://github.com/google/trillian) | v1.8.0, 2026-09-21 | 3761 | Apache-2.0 | badges codecov, godoc, goreportcard; 6 workflows (codeql, lint, govulncheck, scorecard, testes crdb/pg) | servidor Go com rede |
| [Rekor](https://github.com/sigstore/rekor) | v1.5.5, 2026-10-21 | 1233 | Apache-2.0 | 10 workflows (build, codeql, fuzz, scorecard, verify); badge scorecard; 69 issues | servidor Go com rede |
| [immudb](https://github.com/codenotary/immudb) | v1.11.2, 2026-09-03 | 9048 | não confirmada (GitHub: NOASSERTION) | 6 workflows; badges goreportcard e shields; 116 issues | banco Go com rede; licença não confirmada |
| [Tessera](https://github.com/transparency-dev/tessera) | v1.0.4, 2026-07-16 | 257 | Apache-2.0 | badges scorecard e shields; 57 issues | Go; abaixo de 1000 estrelas |
| [Rekor tiles](https://github.com/sigstore/rekor-tiles) | v2.4.1, 2026-10-30 | 51 | Apache-2.0 | `n/v` | Go; abaixo de 1000 estrelas |
| [sigstore-rs](https://github.com/sigstore/sigstore-rs) | v0.15.0, 2026-11-05 | 246 | Apache-2.0 | 5 badges; 58 issues | verificação de assinaturas, foco em Rekor; dependências de rede não auditadas (`sigstore-verify` tem 1.557.140 downloads recentes) |

Estrelas e saúde passam em Trillian, Rekor e immudb, mas o encaixe técnico não: são processos separados.

### Veredito

**Não aptos.** Úteis como fonte de padrões: o formato de checkpoint assinado e as provas de consistência (ver [Gap interno e fundamentos](#gap-interno-e-fundamentos)).

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Descartados por licença

### O que é?

Crates encontrados na busca do crates.io cuja licença exclui o uso em um crate MIT ou Apache-2.0.

### O que cobre?

- `libro` 0.92.0 (2026-04-03): GPL-3.0-only.
- `kedge-ledger` 0.4.0 (2026-07-28): BUSL-1.1.
- `codetopo-store` 0.9.0 (2026-11-02): AGPL-3.0-or-later.

As estrelas deles não foram conferidas, porque a licença já decide.

### Veredito

**Não aptos.**

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Gap interno e fundamentos

### O que é?

O que sobra para implementar dentro do crate `hexlog`, sem biblioteca:

- a cadeia por tenant, com `seq` contíguo e âncora;
- o lote atômico;
- o `verify_chain`: lista ordenada de quebras (não erro), conferência das colunas derivadas contra o corpo e detecção de cauda truncada com `expected_chain_head`.

### O que fundamenta?

- **Cadeia de hash como estrutura**: Haber e Stornetta propuseram em 1991 encadear documentos por hash para datá-los sem confiar em quem guarda. É a origem da cadeia linear.
- **Truncamento e frescor**: Crosby e Wallach (USENIX Security 2009) tratam de logs que detectam adulteração e discutem o que a cadeia sozinha não detecta. É a base do motivo de existir o `expected_chain_head`: sem um head obtido por canal externo, quem controla o banco apaga a cauda ou reescreve tudo. O `vigil-audit` e o `attest-ledger` fazem a mesma ressalva.
- **Head assinado**: a especificação `tlog-checkpoint` do C2SP descreve um checkpoint assinado (origem, tamanho, raiz) como a âncora publicável de um log; serve de modelo se a âncora por tenant precisar ser exportada.
- **Provas Merkle**: se o produto passar a exigir provas de inclusão e consistência, RFC 6962 e RFC 9162 definem a árvore e as provas; `ct-merkle` implementa o estilo 6962.
- **Canonicalização**: RFC 8785 define a regra de ordenar chaves, formatar números e escapar strings que a fórmula do hash usa.
- **Atomicidade**: SQLite documenta commit atômico e transações (`BEGIN IMMEDIATE` reserva a escrita antes de ler o head), o que sustenta o lote atômico e o `seq` contíguo sob concorrência.

### Como aplicar?

Implementar com `sha2` e um canonicalizador JCS. Usar `attest-ledger` (head commitment) e `vigil-audit` (versão de hash por evento, colunas derivadas amarradas) apenas como referência de desenho. Rodar os vetores do RFC 8785 e da porta TS contra `serde_jcs` e `serde_json_canonicalizer` antes de escolher um deles.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Links Úteis

Todos consultados em 2026-11-07.

- [crates.io: vigil-audit](https://crates.io/crates/vigil-audit)
- [GitHub: duncatzat/vigils](https://github.com/duncatzat/vigils)
- [ADR 0002 do vigil-audit](https://github.com/duncatzat/vigils/blob/main/docs/adr/0002-audit-ledger.md)
- [hash.rs do vigil-audit](https://github.com/duncatzat/vigils/blob/main/crates/vigil-audit/src/hash.rs)
- [crates.io: attest-ledger-core](https://crates.io/crates/attest-ledger-core)
- [GitHub: statecrafting/attest-ledger](https://github.com/statecrafting/attest-ledger)
- [crates.io: tamper-audit](https://crates.io/crates/tamper-audit) e [GitHub: WyattAu/auditlog](https://github.com/WyattAu/auditlog)
- [crates.io: audit-trail](https://crates.io/crates/audit-trail) e [GitHub: jamesgober/audit-trail](https://github.com/jamesgober/audit-trail)
- [crates.io: openmoat-audit](https://crates.io/crates/openmoat-audit) e [GitHub: crocodile-labs/openmoat](https://github.com/crocodile-labs/openmoat)
- [crates.io: smooai-audit](https://crates.io/crates/smooai-audit) e [GitHub: SmooAI/audit](https://github.com/SmooAI/audit)
- [crates.io: rs_merkle](https://crates.io/crates/rs_merkle) e [GitHub: antouhou/rs-merkle](https://github.com/antouhou/rs-merkle)
- [crates.io: ct-merkle](https://crates.io/crates/ct-merkle) e [GitHub: rozbb/ct-merkle](https://github.com/rozbb/ct-merkle)
- [crates.io: serde_jcs](https://crates.io/crates/serde_jcs) e [GitHub: l1h3r/serde_jcs](https://github.com/l1h3r/serde_jcs)
- [crates.io: serde_json_canonicalizer](https://crates.io/crates/serde_json_canonicalizer) e [GitHub: evik42/serde-json-canonicalizer](https://github.com/evik42/serde-json-canonicalizer)
- [Trillian](https://github.com/google/trillian), [Rekor](https://github.com/sigstore/rekor), [Rekor tiles](https://github.com/sigstore/rekor-tiles), [Tessera](https://github.com/transparency-dev/tessera), [immudb](https://github.com/codenotary/immudb), [sigstore-rs](https://github.com/sigstore/sigstore-rs)
- [RFC 8785: JSON Canonicalization Scheme](https://www.rfc-editor.org/rfc/rfc8785)
- [RFC 6962: Certificate Transparency](https://www.rfc-editor.org/rfc/rfc6962)
- [RFC 9162: Certificate Transparency 2.0](https://www.rfc-editor.org/rfc/rfc9162)
- [Crosby e Wallach: Efficient Data Structures for Tamper-Evident Logging (USENIX Security 2009)](https://www.usenix.org/legacy/event/sec09/tech/full_papers/crosby.pdf)
- [Haber e Stornetta: How to time-stamp a digital document (J. Cryptology, 1991)](https://link.springer.com/article/10.1007/BF00196791)
- [C2SP: tlog-checkpoint](https://github.com/C2SP/C2SP/blob/main/tlog-checkpoint.md)
- [C2SP: tlog-tiles](https://c2sp.org/tlog-tiles)
- [SQLite: atomic commit](https://www.sqlite.org/atomiccommit.html)
- [SQLite: transações](https://sqlite.org/lang_transaction.html)
- [docs.rs: rusqlite](https://docs.rs/rusqlite)
