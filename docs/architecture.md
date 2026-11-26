# Arquitetura: caixas cinzas do oh-my-hexflow

Segue a decisão de projeto sobre caixas cinzas: caixa cinza é um módulo profundo, de interface pequena e interior delegável ao agente, aninhável. Os limites abaixo saíram do código e dos testes de arquitetura, não de um desenho prévio. Vocabulário: **module**, **interface**, **depth**, **seam**, **adapter** como na skill `codebase-design`; termos de domínio como em `GLOSSARY.md`. "Interface" inclui invariantes e ordem de chamada, não só a assinatura.

## 3. Caixas cinzas

### 3.1 Núcleo: `hexlog` (facade `Hexlog`)

**Profundidade.** Funda.

### 3.2 Aninhada: `hexlog::engine`

**Profundidade.** O seam se paga porque é o que o teste `it_should_not_import_store_or_catalog_from_the_engine_sources` usa para manter o engine sem SQL.

### 3.5 Núcleo do domínio: `omhl::facade` (`Omhl`)

**Profundidade.** Média. Pelo teste da exclusão, sumir com a facade espalharia a guarda e o gate por todos os chamadores; é por isso que ela existe, mas ela é mais larga do que funda.

### 3.6 Aninhada: `omhl::gate`

**Profundidade.** Funda.

### 3.7 Aninhada: `omhl::guard`

**Profundidade.** É o maior consumidor do vocabulário de tipos (seção 5, item 1).

### 3.8 Aninhada: `omhl::pr_gate`

**Profundidade.** Média: política em forma de função. Interface e interior têm tamanho parecido; é aceitável porque é a tabela de política do plugin e muda por motivo próprio.

### 3.9 Aninhada: `omhl::graphs`

**Profundidade.** Funda como função.

### 3.10 Aninhada (folha): `omhl::native` e `omhl::probe`

- `native`: Não é módulo profundo: é dado versionado com o plugin (decisão 16).
- `probe`: Seam real.

### 3.11 Gateway de consumo: `omhl::app`

**Interface.** Só `Busy` é repetido (nada foi gravado nesse caso).

**Catálogo de erros.** Todo código estável ganha construtor nomeado, mesmo com 1 uso (os códigos são contrato externo: JSON da CLI e do hook; o texto `agent` e `human` do erro mostra o mesmo `code` literal).

### 3.12 Gateway de consumo: `omhl::adapters`

**Papel.** Traduz um protocolo externo (linha de comando, evento de hook) em `Action` e imprime o resultado; não conhece o domínio.

**Profundidade.** Funda por adapter: uma função cada.

### 3.13 Aninhada: `omhl::extensions`

**Papel.** Lê as extensions TOML de `.omhl/`, valida e calcula o que mudaria no tenant, sem gravar nada.

### 3.14 Aninhada: `omhl::projection`

**Profundidade.** Funda.

### 3.16 Suporte: `omhl::ports` e `omhl::project`

- `ports`: adapters de produção. Adapters finos de seam real. Os adapters dos gates mecânicos estão na seção 3.22.
- `project`: descoberta do projeto. Rasa: metade é lista de constantes e helpers compartilhados por `extensions` e `projection` (item 7 da seção 5).

### 3.19 Aninhada: `omhl::mechanical::sweep`

**Profundidade.** Funda e pura.

### 3.20 Seam: `omhl::runner`

**Profundidade.** É só o seam.

### 3.21 Seam: `omhl::byproduct`

**Profundidade.** É só o seam.

### 3.22 Adapters novos de `omhl::ports`

- `ports/pipes`: São `thread::spawn` soltas; assim um descendente fugido que segura o cano não prende o runner.

### 3.23 Mudanças nas caixas existentes

- **`SnapshotProbe` e `DevelopMark`** (`probe.rs`). A trait tinha um método, `probe`. A marca da `develop` ganhou a trait própria `DevelopMark`, porque a sonda de leitura não escreve.

### 3.24 Fluxo de diretivas e premissas: `omhl::workflow` e as ports novas

O comportamento está em [directives-flow.md](directives-flow.md). Aqui fica só onde cada parte mora.

#### O hook `agent-role`

- `app/agent_role.rs`: Não abre o banco.

## 4. Regras de dependência e o teste que trava cada uma

### As skills do plugin (`crates/omhl/tests/e2e/skills/`)

O `role_view` (`--lib`, `src/test_support/role_view.rs`) trava cada skill `role-<papel>` contra o `omhl-allow` do agente e o `omhl-cycle` do `cycle.md`, e a tabela "quem usa" das páginas de comando contra a projeção dos agentes. Por ser `--lib`, roda no gate local; os testes de `tests/e2e/skills/` só rodam no CI. Decisão no [ADR 0011](adr/0011-skills-por-papel-e-entrevista-antes-do-work-item.md).

O `skill_examples` executa `check sweep`, que varre o `/proc` inteiro e vale para qualquer raiz da máquina. Pelo mesmo motivo ele pega o flock `omhl-process-sweep.lock`: sem ele, a varredura de um teste poderia matar o processo de outro. O flock só protege contra outro `cargo` concorrente; os binários de um mesmo `cargo test` já rodam em sequência.

## 5. Pontos rasos ou com vazamento

"Corte" é o que recomendo fazer; "dívida" é o que aceito até haver motivo.

1. **Vocabulário de tipos espalhado (corte).** Trade-off: `graphs` ganha interface; em troca vira o único dono do vocabulário, que o `gate` já prova que dá para isolar.

2. **`app` entra no domínio por mais de uma porta (corte barato, decisão sua no segundo passo).** Corte barato: um teste que fixe o conjunto permitido de `app`. Corte maior, só se valer: mover a montagem do atestado para dentro de `Omhl`, o que acrescenta uma operação à facade e tira o conhecimento de `guard` de `app`.

3. **Paginação e constantes duplicadas (corte).** Corte: `Omhl` expõe uma leitura de todos os nodes de um tipo reaproveitando `HexlogSource`. Trade-off: uma operação a mais na facade, em troca de um só laço.

4. **A sonda do git vive em mais de uma camada (dívida).** Aceitável porque `evaluate_pr_gate(tenant, work_item, &snapshot)` recebe o snapshot explícito. Vira corte se aparecer um caso em que as duas leituras discordem.

5. **Nome e formato de porta dentro de `app` (corte).** Corte: o adapter traduz o payload para um struct neutro e passa o `Caller` pronto; `app` deixa de nomear a porta. Trade-off: não medido além de mover os nomes de `Caller` e o parse do payload.

6. **Contrato de saída sem tipo (dívida).** O gatilho do corte disparou com o renderizador de `load` (`adapters/text/load.rs`), o segundo leitor do relatório de `app/load.rs`, e a dívida foi aceita nesta entrega: o `LoadReport` tipado fica como follow-up. O teste de folhas é de `tests/`: o gate local `--lib` não o cobre, e depois do PR só o CI o roda. O corte (um `LoadReport` tipado em `app`) vale quando o teste de folhas reprovar por chave renomeada do `load` uma vez, ou quando aparecer um terceiro leitor.

7. **Helpers no lugar errado (corte pequeno, em parte feito).** Corte: `render::sort_by_id` vai para `project` (que já hospeda `text.rs`).

8. **Superfície pública do `hexlog` maior que o consumo (dívida consciente).** A causa é a regra de que os testes de integração usem só a API pública (`it_should_keep_integration_tests_on_the_public_api`): o que os testes de adulteração precisam vira público. Cortar exige uma feature de cargo para test support.

9. **Helpers de teste copiados entre `src/test_support` e `tests/support` (dívida aceita).** O teste unitário de `src/**/tests.rs` não alcança `tests/support`, então os helpers existem nas duas pastas. Feature de cargo e `#[path]` foram descartados no plano da `perf3` (D4; decisão `perf3-test-support-copiado`). Custo: mudar um helper vale em dois arquivos, e nenhum teste confere que as cópias não derivam. Gatilho do corte: a primeira deriva de corpo achada, ou o fluxo de entrevista de fluxos que o plano reserva para deduplicar. Corte: um arquivo único de helper, junto com o corte do item 8.

10. **Binário anterior sobre banco com sessão não é suportado (limite aceito).** O corpo do registro recusa campo desconhecido: um `omhl` que não conhece `session` falha `get` e `list` nesses registros com o detalhe `invalid-body`, e `verify` acusa cada um (`it_should_read_a_body_with_an_unknown_top_level_field_as_invalid_body`). Reverter a mudança não devolve a leitura: o caminho de volta é instalar um binário que conhece o campo, nunca apagar nem reescrever registros. O procedimento de instalar o binário novo depois do merge está no [CONTRIBUTING.md](../CONTRIBUTING.md#binário-novo-depois-do-merge).
