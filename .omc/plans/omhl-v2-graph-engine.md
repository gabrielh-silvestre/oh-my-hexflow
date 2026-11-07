# Plano inicial: motor de grafos em Rust e plugin de desenvolvimento opinativo (v2, rascunho)

- **Status:** rascunho de alinhamento, atualizado em 2026-11-07 com as decisões da entrevista. Nada aprovado, nada iniciado.
- **Relação com o v1:** muda a direção de [omhl-v1-vertical-slice.md](omhl-v1-vertical-slice.md), que fica intacto como histórico.
- **Nomes:** motor `hexlog` (Rust), plugin `oh-my-hexflow`, binário `omhl`.

## Documentos de apoio

- Modelo do Hexlog atual: [hexlog-current-model.md](../research/hexlog-current-model.md)
- Vocabulários: veredito do júri e tabela de termos em [jury-verdict.md](../research/vocab/jury-verdict.md); notas de origem na mesma pasta
- Candidatos à base nativa (mineração de sessões): [native-base-candidates.md](../research/native-base-candidates.md)

## 1. Decisões do dono

1. **Hexlog vira um motor de grafos agnóstico, reescrito em Rust.** Cuida de entidades, tipos, relações, cadeia de registros e consulta. Não sabe o que é decisão nem PRD.
2. **O plugin é opinativo e fica em cima do motor.** Cada grafo de domínio define tipos e campos estruturados; fluxos que seriam skills passam a ser grafos (PRD, planos e o fluxo do Ralph depois).
3. **Grafos da v1: decisões e checks.** O grafo de checks reúne a verificação mecânica: etapas (`check`) que valem antes de certos eventos. O nó `check` tem `when`, `how` e `blocking`.
4. **Na v1 o conteúdo é só texto de instrução para o agente.** Nada dispara nem bloqueia. O agente lê e escolhe como se preparar.
5. **Entrega por injeção no contexto**, por padrão no `SessionStart`, só em projetos que usam o plugin. Outros momentos "dependem" e ficam para depois.
6. **Ids semânticos.** O id é o nome estável do nó (ex.: `gates.before-commit.run-tests`). Cada revisão de um registro do projeto tem id técnico próprio na cadeia.
7. **Interface:** CLI e MCP no mesmo binário, agnósticos por Facade e Adapter.
8. **Caixas cinzas (Matt Pocock):** módulos profundos, com interface pequena e interior delegado ao agente, aninháveis. Os limites saem da análise do código (passo 7), não de antemão.
9. **Armazenamento:** um SQLite global do usuário, com projetos como tenants. A cadeia de hash é por tenant. O SQLite é a fonte de verdade de tudo.
10. **Opt-in e tenant:** um arquivo marcador no repo declara o tenant. Sem arquivo, não há injeção.
11. **Repositório:** monorepo `oh-my-hexflow` com workspace Cargo (crate do motor `hexlog` e crate do binário e plugin `omhl`).
12. **Compatibilidade:** recomeça do zero, sem migração. O Hexlog em TypeScript fica como vitrine de referência.
13. **Grafo de decisões** herda os tipos **premissa, diretriz e decisão**. As relações entre entidades são registros próprios. A evidência não é um tipo externo: é embutida na decisão (decisão 28).
14. **Nó `check`:** `when` (quando executar), `how` (como executar), `blocking` (informativo na v1). O `when` é `{vocabulary, event, matcher?}`: o vocabulário do host como está (veredito W-A do júri). Na v1 só existe o vocabulário `free`; cada vocabulário absorvido depois (git, claude-code, github...) terá suporte nativo. Não há núcleo comum entre hosts na v1.
15. **Nomes de tipos, relações e revisões:** nomes próprios do domínio, mais `equivalent: <URI>` opcional e informativo (veredito N-A do júri). Nenhum padrão externo entra como dependência de código. O motor valida só a sintaxe da URI e nunca a consulta.
16. **Base nativa:** entidades no código Rust, embutidas no binário, fora da cadeia do tenant e versionadas com o plugin. Conteúdo da v1: esqueleto mais poucos itens reais (candidatos em [native-base-candidates.md](../research/native-base-candidates.md): gates antes de commitar rodar os testes, merge só com CI verde, conferir a ancoragem antes de postar comentários de review; diretrizes de entrevista uma decisão por turno e de política de commit com padrão conservador).
17. **Extensão por projeto: só adiciona, base imutável.** O projeto acrescenta nós e passos ligados aos nativos; nunca altera nem substitui um nativo. Substituição por id fica para um marco seguinte; o id semântico e o `externalId` ficam reservados no modelo.
18. **Projeção em arquivos:** gates, checks e diretrizes (registros atemporais) são projetados em arquivos no repo para versionamento. O SQLite continua sendo a verdade. Direção: **só do banco para o arquivo**; o arquivo é gerado e não é editado à mão. Formato: **YAML puro, um arquivo por nó**, com o texto longo de instrução como referência externa. Ficam em `.omhl/`. **Revisada pela decisão 34:** os YAML deixam de ser projeção; só o Markdown e os demais arquivos gerados continuam sendo.
19. **Subagentes só em sonnet ou haiku**, exceto quando o dono pedir outro modelo.
20. **Ordem dos passos:** relação explícita entre passos, gravada como registro (ex.: `after`, `before`). Um projeto liga um passo novo a um nativo por essa relação.
21. **O id semântico é o `target`** do Hexlog atual. Um nome identifica uma linhagem; o id técnico existe só por revisão.
22. **Nomes de nó:** o nó de instrução com `when`, `how` e `blocking` se chama **`check`**. **`gate`** continua sendo a consulta avaliável do Hexlog (passou ou falhou), que a v1 porta.
23. **Arquivo marcador:** `.omhl.toml` na raiz do repo, com o tenant. Sem o arquivo, não há injeção.
24. **`gate` avaliável:** a v1 porta as 4 perguntas do Hexlog como estão (aprovado, ocorreu, sem pendência, sem contradição aberta).
25. **Texto longo de instrução:** fica no SQLite como parte do registro (com hash) e é projetado em um `.md` irmão do YAML, **em uma subpasta própria** dentro de `.omhl/`. O YAML do nó só referencia o `.md`. Nome da subpasta e layout exato a definir.
26. **Regeneração da projeção:** configurável (a cada escrita, sob demanda, no `SessionStart` ou após `git pull`). O padrão é um hook do git (`post-merge`) que regenera os arquivos na branch de trabalho depois de um `git pull`. **Revisada pela decisão 34:** vale só para as projeções (Markdown e outros), não para os YAML.
27. **Um único usuário por tenant na v1.** Se um arquivo de projeção trazido por `git pull` divergir do banco, o motor avisa e regenera a partir do banco. O trabalho de outra pessoa nesses arquivos não é preservado; colaboração em time fica para depois. **Revisada pela decisão 34:** vale só para as projeções (Markdown e outros), não para os YAML.
28. **Evidência embutida na decisão.** A evidência vive dentro da própria decisão, não como entidade externa. O que é externo são os **gates mecânicos**: determinísticos e passíveis de **re-aprovação**.
29. **O PR entra na v1 como o primeiro gate nativo.** A base traz o tipo **`unit`** (um PR, com estado aberto e fechado) e o primeiro `gate` avaliável nativo sobre ele (pré-PR e fechamento). `open_unit` e `close_unit` deixam de ser tools do v1 antigo e passam a ser a forma de registrar e consultar essa entidade.
30. **Aprovação de gate mecânico é determinística e amarrada ao estado.** Se o check diz que passou, refazer o check no mesmo estado deve dar o mesmo resultado. Re-aprovar é reproduzir, não opinar.
31. **O estado avaliado vive no núcleo do motor, como campo tipado.** O único tipo de estado na v1 é o **SHA do commit** do git. Outros tipos de estado serão adicionados diretamente no núcleo, e cada um precisa ser **tão seguro de reproduzir quanto o git**. Consequência aceita: o núcleo conhece o git, e um estado com alterações não commitadas não pode ser aprovado.
32. **Estados do `unit`: aberto e fechado.** O fechamento é o momento em que o `gate` nativo de PR é avaliado. Estados mais ricos (revisão, mesclado) ficam para depois.
33. **Ordem entre `check`s: só a relação `after`.** `before` é a leitura inversa, sem registro próprio. Sem relação entre dois checks, eles são independentes (podem ser paralelos).
35. **Linguagem ubíqua:** o vocabulário do projeto está em [GLOSSARY.md](../../GLOSSARY.md) e prevalece sobre os nomes das decisões 1 a 34 e do plano do passo 3 onde divergirem. Mudanças em relação aos nomes anteriores: record (antes elo ou registro), relation e relation type (antes edge e relação), work item (antes unit), attestation (antes approval), snapshot (antes state), latest record (antes tip), chain head (antes head), lineage (antes subject ou linhagem), host (só o ambiente do agente; o consumidor é o `omhl`). Revisada por 2 critics adversariais (padrões e colisões).
34. **Os YAML em `.omhl/` são extensions, não projeções.** São escritos à mão, versionados e carregados no banco como nodes do projeto (arquivo para banco). São a extensão de comportamento da decisão 17. O texto longo de uma extension vive em Markdown escrito à mão, referenciado pelo YAML, e faz parte da extension. Tudo que é versionado e relacionado ao YAML é estritamente validado e opinativo. Os demais arquivos gerados continuam sendo projeções (banco para arquivo, nunca editados à mão). As decisões 18, 26 e 27 passam a valer só para as projeções, e a decisão 25 deixa de valer para extensions. Efeito: um clone novo ou uma segunda máquina recarrega o banco a partir das extensions. A projeção passa a ser uma vista gerada sob demanda a partir do banco, em arquivos só de leitura; cada projeto pode querer projeções diferentes, e a primeira é o estado atual por unidade de trabalho (tudo que está vigente). Em aberto: o formato e o comando dessa projeção, e as regras da validação estrita.

## 2. Escopo da v1

**Entra:** motor (tipos e relações definidos por dados, ids semânticos, revisões, cadeia de hash por tenant, consulta, `gate` avaliável portado do Hexlog); grafo de decisões (com evidência embutida na decisão); grafo de checks; entidade `unit` do PR e o primeiro `gate` nativo de PR; base nativa em Rust; extensão por adição; adapters CLI, MCP e hook de `SessionStart`; arquivo marcador `.omhl.toml`; projeção de checks, gates e diretrizes em arquivo.

**Fora (marcos futuros):** execução e bloqueio de gates (o que era HMAC, guard, `omhl check`, `Stop` como freio); juiz Haiku; observabilidade e `omhl report`; grafo de PRD e de planos; substituição de nó nativo por id; vocabulário `core` entre hosts; resultado de gate (in-toto, SARIF); injeção por prompt ou contextual.

## 3. Esboço do código (hipótese, para análise)

| Módulo | Responsabilidade | Interface esperada |
|---|---|---|
| `engine` | registros, revisões, cadeia por tenant, consulta, regra de um vigente por linhagem | Facade única do núcleo |
| `schema` | tipos e relações como dados, validação | carregar e validar definições |
| `store` | persistência em SQLite global, tenants, transação por lote, FTS5 | anexar e ler registros |
| `native` | base nativa como entidades Rust (gates e diretrizes) | listar nós nativos e sua versão |
| `graphs/decisions`, `graphs/checks` | tipos (`premissa`, `diretriz`, `decisão`; `check`), relações e regras de cada grafo, mais o `gate` avaliável | definições |
| `render` | transforma nós aplicáveis (nativos mais do projeto) em texto de injeção | nós -> texto |
| `project` | arquivo marcador, projeção em arquivo | ler tenant, gerar arquivos |
| `adapters/cli`, `adapters/mcp`, `adapters/hook` | traduzem entrada e saída, sem lógica de domínio | chamam a Facade |

Do relatório do Hexlog atual, vale portar: cadeia sha256 sobre JCS com `seq` contíguo e `expectedHead`, lote atômico de 1 a 50 com `@alias`, idempotência por `key` mais `fingerprint`, regra de um vigente por linhagem (`FORK_REJECTED`). Saem com o SQLite: lock por diretório, escrita atômica manual, teto de 64 MiB e o cache de busca.

## 4. Critérios de aceite (rascunho)

1. Registrar um nó do projeto com id semântico e consultá-lo pelo id devolve a revisão vigente.
2. Adicionar um nó ou passo do projeto ligado a um nó nativo não altera o nativo; gravar um registro com id semântico de nó nativo é recusado.
3. A verificação da cadeia de um tenant detecta qualquer registro adulterado; a cadeia de um tenant não depende da de outro.
4. Projeto sem arquivo marcador: o `SessionStart` não injeta nada.
5. Projeto com marcador: injeta a base nativa mais os nós do projeto.
6. CLI e MCP devolvem o mesmo resultado para a mesma consulta (ambos passam pela Facade).
7. Adicionar um terceiro tipo de grafo não exige mudar o `engine`.
8. A projeção em arquivo de gates e diretrizes é determinística: o mesmo estado do banco gera os mesmos bytes.

## 5. Pontos abertos (precisam do dono)

- **Campos do `.omhl.toml`:** além do tenant, o que mais entra.
- **Layout de `.omhl/`:** nome da subpasta dos `.md`, pasta por tipo de nó ou plana, e convenção de nome de arquivo derivada do id semântico.
- **Isolamento entre tenants** no SQLite global e o que acontece se o usuário mover ou renomear um projeto.
- **Teste do `when` bruto:** medir se o agente lê bem o `when` de host (premissa mais arriscada do júri).
- **Licença:** embutir termos do schema.org ou de outros padrões em conteúdo da base nativa.
- **A entidade `unit` do PR:** campos, relação com decisões e checks, e a definição do `gate` nativo de PR (o que ele avalia e quando se diz "aprovado"). Os estados já estão decididos (aberto e fechado).
- **Forma do campo de estado no núcleo:** `{kind, id}` (ex.: `git-commit`) ou campo único; critério mínimo para aceitar um novo tipo de estado ("tão reproduzível quanto o git"); validação do SHA.

## 6. Passos

1. ~~Ler o Hexlog atual~~ (feito: relatório). ~~Pesquisa de vocabulários~~ (feito: júri). ~~Mineração de sessões para a base nativa~~ (feito).
2. **Fechar os pontos abertos** da seção 5, um por vez, com o dono.
3. **Núcleo do `engine` e `store`** com testes dos critérios 1, 3 e 7.
4. **Grafos de decisões e de gates** e a **base nativa** em Rust (critério 2).
5. **Arquivo marcador, render e adapters** CLI, MCP e hook (critérios 4 a 6).
6. **Projeção em arquivo** (critério 8).
7. **Análise do código produzido e diagrama da arquitetura.** Delimitar as caixas cinzas (núcleo, gateways de consumo e caixas aninhadas onde um contexto crescer), com a interface de cada uma.
