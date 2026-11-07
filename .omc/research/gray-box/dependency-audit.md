# Auditoria das dependências do crate hexlog

## Tabela de Conteúdos

- [Critério de aptidão](#critério-de-aptidão)
- [Dependências aprovadas](#dependências-aprovadas)
- [Dependências reprovadas](#dependências-reprovadas)
- [Alternativas avaliadas](#alternativas-avaliadas)
- [Links Úteis](#links-úteis)

---

## Critério de aptidão

### O que é?
A regra do dono para aceitar uma biblioteca: no mínimo **1000 estrelas** no GitHub, atualizações relevantes recentes e bom estado de saúde (documentação, testes e badges). A auditoria cobre as dependências diretas do plano `.omc/plans/omhl-v2-core-step3.md` (linha 191), com dados de **2026-11-07**.

### O que faz?
Separa as dependências em aptas e não aptas. Estrelas são as do repositório que o crate aponta no crates.io; em monorepo, vale o repositório inteiro (`sha2` mora em `RustCrypto/hashes`, `time` em `time-rs/time`). "Recente" foi lido como commit no branch padrão nos últimos meses, mais release publicada. Badges foram checados por padrão de URL no README (shields, `badge.svg`, codecov), não visualmente.

### Como refazer?
Os dados saíram de duas APIs:

```bash
# repositório, versão, data e downloads recentes
curl -s -A "research-client" https://crates.io/api/v1/crates/<crate>

# estrelas, último push, arquivado, branch padrão
gh api repos/<owner>/<repo>
gh api repos/<owner>/<repo>/commits/<branch> --jq '.commit.committer.date'
```

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Dependências aprovadas

### O que é?
Dez crates diretos que passam no critério: oito normais e dois de desenvolvimento (`proptest`, `tempfile`).

### O que faz?
Cobrem serialização, hash, banco, UUID, erros, datas, URLs e testes de propriedade. Todos têm commit nos últimos meses, documentação no docs.rs, testes e CI.

### Tabela
| crate | repositório | estrelas | último commit | última release | docs | testes/CI | badges |
|---|---|---|---|---|---|---|---|
| `serde` | [serde-rs/serde](https://github.com/serde-rs/serde) | 11046 | 2026-10-16 | 1.0.229 (2026-07-18) | docs.rs, serde.rs | `test_suite`, `ci.yml` | crates.io, shields |
| `serde_json` | [serde-rs/json](https://github.com/serde-rs/json) | 5661 | 2026-10-16 | 1.0.154 (2026-10-11) | docs.rs | `tests`, `fuzz`, `ci.yml` | crates.io, shields |
| `sha2` | [RustCrypto/hashes](https://github.com/RustCrypto/hashes) (monorepo) | 2291 | 2026-10-23 (pasta `sha2`) | 0.11.1 (2026-10-12) | docs.rs | `tests`, `benches`, CI | CI, crates.io |
| `rusqlite` | [rusqlite/rusqlite](https://github.com/rusqlite/rusqlite) | 4467 | 2026-11-06 | 0.41.1 (2026-10-28) | docs.rs | `tests`, `examples`, `main.yml` | CI, codecov |
| `uuid` | [uuid-rs/uuid](https://github.com/uuid-rs/uuid) | 1249 | 2026-11-06 | 1.29.1 (2026-11-06) | docs.rs | `tests`, `fuzz`, `ci.yml` | CI, crates.io |
| `thiserror` | [dtolnay/thiserror](https://github.com/dtolnay/thiserror) | 5590 | 2026-10-16 | 2.0.21 (2026-09-23) | docs.rs | `tests`, `ci.yml` | crates.io, shields |
| `time` | [time-rs/time](https://github.com/time-rs/time) (monorepo) | 1349 | 2026-10-16 | 0.3.55 (2026-08-01) | docs.rs (campo vazio no crates.io) | `time/tests`, `build.yaml` | CI, codecov |
| `url` | [servo/rust-url](https://github.com/servo/rust-url) (monorepo) | 1585 | 2026-10-30 | 2.5.8 (2026-01-05) | docs.rs | `url/tests`, `fuzz`, `main.yml` | CI, codecov |
| `proptest` (dev) | [proptest-rs/proptest](https://github.com/proptest-rs/proptest) | 2276 | 2026-10-10 | 1.12.0 (2026-10-10) | docs.rs, livro | `proptest/tests`, `rust.yml` | CI, docs.rs |
| `tempfile` (dev) | [Stebalien/tempfile](https://github.com/Stebalien/tempfile) | 1472 | 2026-10-30 | 3.27.0 (2026-03-11) | docs.rs | `tests`, `ci.yml` | CI, crates.io |

`url` tem a release mais antiga (2026-01-05, cerca de dez meses), mas segue com commits recentes.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Dependências reprovadas

### O que é?
Três crates que ficam abaixo de 1000 estrelas. Os três têm documentação, testes, CI e atividade recente: a reprovação é só pelo corte de estrelas.

### O que faz?
Cada uma pede uma decisão do dono.

### Tabela
| crate | repositório | estrelas | último commit | última release | motivo | alternativa apta |
|---|---|---|---|---|---|---|
| `serde_json_canonicalizer` | [evik42/serde-json-canonicalizer](https://github.com/evik42/serde-json-canonicalizer) | 23 | 2026-10-13 | 0.4.1 (2026-10-13) | 23 estrelas | sem alternativa apta |
| `jsonschema` | [Stranger6667/jsonschema](https://github.com/Stranger6667/jsonschema) | 833 | 2026-11-06 | 0.60.2 (2026-10-26) | 833 estrelas, a 167 do corte | sem alternativa apta |
| `proc-macro2` (dev) | [dtolnay/proc-macro2](https://github.com/dtolnay/proc-macro2) | 946 | 2026-08-25 | 1.0.107 (2026-07-19) | 946 estrelas | sem alternativa |

Notas por crate:

- **`serde_json_canonicalizer`:** é o maior canonicalizador JCS (RFC 8785) em Rust no GitHub e no crates.io. Saídas: abrir exceção ao critério ou implementar o RFC 8785 sobre `serde_json`. Ordenar chaves é simples; a formatação de números no estilo ECMAScript é a parte difícil.
- **`jsonschema`:** a única biblioteca com mais de 1000 estrelas na busca é `supabase/pg_jsonschema` (1207), uma extensão Postgres e não um validador utilizável como biblioteca.
- **`proc-macro2`:** provavelmente é dependência transitiva (`serde_derive`, `thiserror`). Se estiver como direta no plano, vale removê-la; se não, é o crate padrão de proc macros.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Alternativas avaliadas

### O que é?
Crates concorrentes dos dois reprovados de domínio (JCS e JSON Schema), medidos pelo mesmo critério.

### O que faz?
Todas reprovam, então não resolvem a lacuna.

### Tabela
| crate | repositório | estrelas | último push | última release | veredito |
|---|---|---|---|---|---|
| `serde_jcs` | [l1h3r/serde_jcs](https://github.com/l1h3r/serde_jcs) | 8 | 2026-03-25 | 0.2.0 (2026-03-25) | reprovada |
| `boon` | [santhosh-tekuri/boon](https://github.com/santhosh-tekuri/boon) | 120 | 2026-11-01 | 0.6.1 (2025-01-07) | reprovada |
| `valico` | [s-panferov/valico](https://github.com/s-panferov/valico) | 305 | 2024-03-07 | 4.0.0 (2023-05-13) | reprovada, parada |
| `jsonschema-valid` | [mdboom/jsonschema-valid](https://github.com/mdboom/jsonschema-valid) | 25 | 2026-07-28 | 0.5.2 (2023-11-08) | reprovada |

Busca de outros JCS em Rust (crates.io por `jcs` e `canonical json`; GitHub por `JCS canonicalization rfc8785` e `json canonicalization`): o melhor resultado é o próprio `serde_json_canonicalizer`. Os demais têm 1 estrela ou nenhuma (por exemplo `lattice-substrate/jcs-schubfach-rs`). O crates.io não lista repositório para `valico`; o repositório foi achado por busca no GitHub.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Links Úteis

Todos consultados em 2026-11-07.

- [crates.io API: serde](https://crates.io/api/v1/crates/serde): modelo da chamada usada para os 17 crates (trocar o nome).
- [crates.io: serde_json_canonicalizer](https://crates.io/crates/serde_json_canonicalizer)
- [crates.io: serde_jcs](https://crates.io/crates/serde_jcs)
- [crates.io: jsonschema](https://crates.io/crates/jsonschema)
- [crates.io: boon](https://crates.io/crates/boon)
- [crates.io: valico](https://crates.io/crates/valico)
- [crates.io: jsonschema-valid](https://crates.io/crates/jsonschema-valid)
- [GitHub: serde-rs/serde](https://github.com/serde-rs/serde)
- [GitHub: serde-rs/json](https://github.com/serde-rs/json)
- [GitHub: evik42/serde-json-canonicalizer](https://github.com/evik42/serde-json-canonicalizer)
- [GitHub: RustCrypto/hashes](https://github.com/RustCrypto/hashes) (monorepo de `sha2`)
- [GitHub: rusqlite/rusqlite](https://github.com/rusqlite/rusqlite)
- [GitHub: Stranger6667/jsonschema](https://github.com/Stranger6667/jsonschema)
- [GitHub: uuid-rs/uuid](https://github.com/uuid-rs/uuid)
- [GitHub: dtolnay/thiserror](https://github.com/dtolnay/thiserror)
- [GitHub: time-rs/time](https://github.com/time-rs/time)
- [GitHub: servo/rust-url](https://github.com/servo/rust-url)
- [GitHub: proptest-rs/proptest](https://github.com/proptest-rs/proptest)
- [GitHub: Stebalien/tempfile](https://github.com/Stebalien/tempfile)
- [GitHub: dtolnay/proc-macro2](https://github.com/dtolnay/proc-macro2)
- [GitHub: l1h3r/serde_jcs](https://github.com/l1h3r/serde_jcs)
- [GitHub: santhosh-tekuri/boon](https://github.com/santhosh-tekuri/boon)
- [GitHub: s-panferov/valico](https://github.com/s-panferov/valico)
- [GitHub: mdboom/jsonschema-valid](https://github.com/mdboom/jsonschema-valid)
- [GitHub: supabase/pg_jsonschema](https://github.com/supabase/pg_jsonschema): extensão Postgres, descartada como alternativa.
- [RFC 8785 (JCS)](https://www.rfc-editor.org/rfc/rfc8785): a especificação que o canonicalizador implementa.
