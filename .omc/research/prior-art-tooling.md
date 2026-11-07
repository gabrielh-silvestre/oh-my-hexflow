# Prior art: ferramentas, hooks do Claude Code e estratégia de fork

Fonte: relatório do agente `sherazade-document-specialist-prior-art-tooling`, verificado em 2026-10-30 (Claude Code v2.1.314). O que ele não conseguiu verificar está marcado "não verificado". Spec de origem: [deep-interview-oh-my-hexflow-design.md](../specs/deep-interview-oh-my-hexflow-design.md). Frentes irmãs: [prior-art-frameworks.md](prior-art-frameworks.md) e [prior-art-verification.md](prior-art-verification.md).

## 1. Rust (`omhl`)
| Item | Versão, licença | Veredito |
|---|---|---|
| clap | 4.6.7, MIT/Apache | Usar |
| serde / serde_json | 1.0.229 / 1.0.154 | Usar (tipos de entrada e saída dos hooks) |
| schemars | 1.2.3, MIT | Usar: gera o JSON Schema do contrato de check a partir dos tipos Rust |
| jsonschema | 0.60.2, MIT | Evitar por ora: serde tipado já valida `status/subject/details` |
| thiserror / anyhow | 2.0.21 / 1.0.104 | Usar |
| miette | 7.6.0 (último release 2025-04) | Evitar (parado ~18 meses) |
| tracing | 0.1.44 | Opcional; um logger em arquivo basta |
| sha2 + hmac | 0.11.1 / 0.13.0 | Usar: ambos em digest 0.11, compatíveis |
| blake3 | 1.8.7 | Evitar: o Node não tem blake3 nativo e o servidor confere token e hash |
| serde_json_canonicalizer | 0.4.1, MIT | Usar para o hash do conteúdo da diretiva. O Hexlog usa o npm `canonicalize` 5.1.0 (RFC 8785); **a equivalência entre os dois não foi testada** |
| fs4 / atomic-write-file | 1.1.0 / 0.3.1 | Usar (lock e escrita atômica de `.omhl/state`); fs2 está parado desde 2018 |
| process-wrap | 10.0.1 | Usar (timeout matando o grupo de processos do `omhl check`) |
| assert_cmd, insta, insta-cmd, proptest, tempfile, assert_fs, trycmd, cargo-nextest | versões atuais | Usar nos testes |
| cargo-dist | v0.33.1 (GitHub; crates.io ainda em 0.32.0) | Usar; instalar pelo GitHub |
| cargo-binstall | 1.26.1, GPL-3.0 | Usar como ferramenta (sem link) |
| rmcp | 3.8.0 | Não precisa: o `omhl` nunca fala MCP |

**Hooks do Claude Code em Rust:** não há crate maduro com tipos de hook, estado de Stop ou recibo. `claude-hooks` 0.1.2 só instala hooks no settings; `claude-codes` cobre o protocolo stream-json; `agent-config` e `agentgear` são instaladores; `destructive_command_guard` (6,2k estrelas, licença "Other") é só referência; `cupcake` (OPA/Rego) é pesado. Conclusão: escrever ~150 linhas de serde, sem `deny_unknown_fields`, com `tool_input` como `serde_json::Value`, e fixtures capturadas de um Claude Code real.

## 2. Claude Code (docs oficiais)
- **(i) PreToolUse altera args de tool MCP: confirmado, com ressalva.** `updatedInput` substitui o objeto inteiro (desde 2.0.10). Desde a 2.1.300 é validado: chave desconhecida é descartada e entrada rejeitada pela tool nega a chamada. Consequência: o campo do token precisa estar declarado no `inputSchema` de `create_directive`, e o hook deve sobrescrevê-lo sempre.
- **(ii) PostToolUse recebe o resultado da tool MCP: confirmado em parte.** Recebe `tool_input`, `tool_response` e `mcp_server`, mas o formato exato do `tool_response` de tool MCP não é documentado. Precisa de teste de sonda.
- **(iii) Stop bloqueia: confirmado, com teto.** `{"decision":"block","reason":...}` ou exit 2; `stop_hook_active` true quando já continua. Teto de 8 continuações seguidas (v2.1.143), ajustável por `CLAUDE_CODE_STOP_HOOK_BLOCK_CAP`.
- **(iv) Plugin declara e versiona hooks: confirmado.** `hooks/hooks.json` e chave `hooks` do manifesto (mescladas); `${CLAUDE_PLUGIN_ROOT}` e `${CLAUDE_PLUGIN_DATA}`. Versão: `version` do plugin.json > entrada do marketplace > SHA. `claude plugin tag` (2.1.118) cria a tag `<plugin>--v<versão>`. `dependencies` com semver entrou entre 2.1.105 e 2.1.110. `bin/` do plugin entra no PATH do Bash desde a 2.1.91.
- Outras versões: `updatedToolOutput` 2.1.121; `onFailure:"block"` 2.1.295; hook `type:"mcp_tool"` 2.1.118; campo `if` 2.1.85; `args` (exec form) 2.1.139.

**Fatos que mudam o desenho:**
- Hooks do mesmo evento rodam em paralelo, sem ordem; precedência deny > defer > ask > allow. Dois `updatedInput` conflitantes: não documentado.
- **O Stop não fecha em falha:** `onFailure` não vale em Stop/SubagentStop, e estourar o teto de 8 libera o turno. "Stop fecha tudo" só vale como freio; a proteção real fica no PreToolUse com `onFailure:"block"` e no `close_unit`.
- **SessionStart não bloqueia.** O handshake só injeta `additionalContext`; o bloqueio efetivo é um PreToolUse que nega quando a major diverge.
- **Matcher:** tool MCP de usuário é `mcp__hexlog__*`; se o plugin empacotar o servidor, vira `mcp__plugin_<plugin>_hexlog__*`. Usar regex que cubra as duas formas.
- Mid-sessão, o hook de plugin atualizado segue na versão antiga até `/reload-plugins`; o handshake precisa tolerar essa janela.
- A doc trata `commands/` como formato antigo e diz que skills o substituem; decidir se `/omhl-init` nasce como skill. Namespace de comando de plugin: não verificado.
- Hook `mcp_tool` pode chamar tool do fork direto, mas `isError:true` vira erro sem bloqueio (fail-open): só serve ao opcional.
- Não verificado: se o modelo enxerga o `updatedInput` injetado.

## 3. Fork do Hexlog (Node)
Estado atual (`~/personal/hexlog`, v1.2.3): Node ^24.18.1, `@modelcontextprotocol/server` ^2.3.1, zod 4, ajv, canonicalize, shell-quote; esbuild com duas entradas (`server` e `bash-guard`); jest 30, stryker, cobertura mínima de 90%, eslint 10, knip, husky, commitlint, git-cliff. Camadas domain/commands/queries/adapters/mcp, `compose.ts` como raiz, `defineTool` e um `register<X>Tools` por família. Já existe um hook TypeScript (`hook/bash-guard.ts`) que impede ler o diretório de dados via Bash, modelo para o `omhl`.

O número "12 tools" está fixo em `src/mcp/server.ts`, `src/installation.ts`, `test/mcp/tools.spec.ts`, `test/mcp/environment.ts`, `test/mcp/legacy.spec.ts`, `test/skill-coherence.spec.ts` e nos AGENTS.md. É onde o rebase vai conflitar.

**Estratégia recomendada: carry patches rebaseados.** `git subtree` serve para embutir subprojeto, não para acompanhar um repo com patches; quilt/StGit é ferramenta a mais sem ganho sobre rebase com `range-diff`; merge periódico perde o inventário dos patches.
1. Código novo em arquivos novos (ex.: `src/mcp/tools/units.ts`, `src/commands/units.ts`) com um único ponto de registro em `createServer`.
2. Mandar ao upstream (que é seu) uma costura pequena de registro de famílias de tools e fazer os testes derivarem a contagem dessa lista, em vez de fixar 12.
3. `git rerere` ativo e `git range-diff` a cada rebase.
4. Commits com rótulo `carry:` / `drop:` (como `UPSTREAM: <carry>:` do openshift/kubernetes) e um `PATCHES.md` de inventário.
5. Versionar o fork como `vX.Y.Z-omhl.N`, com a major igual à do `omhl` e do plugin.

Limite: os ADRs do Hexlog fixam o contrato de 12 tools; ampliá-lo exige ADR novo, decisão sua.

## 4. Ferramentas prontas
- **just** 1.59.0 (CC0): ponto único de entrada (`just check`, `just test`); serve de comando para o check "testes locais".
- **mise** v2026.10.24: fixa Node 24.18.1 e Rust 1.99.1.
- **lefthook** v2.2.2: só no repositório Rust; o fork já tem husky e lint-staged (não ter dois gerenciadores).
- **act**: evitar como prova de CI (roda em Docker; o gate exige o CI real no SHA).
- **gh** v2.103.3, testado: (a) `gh pr checks` recebe número, URL ou branch, **não SHA**; (b) CI por commit funciona com `gh api repos/{o}/{r}/commits/{sha}/check-runs` e `gh pr view --json headRefOid,statusCheckRollup`; (c) `gh pr view --json` não expõe `reviewThreads`, mas a consulta GraphQL `reviewThreads { nodes { isResolved isOutdated } }` funcionou.
- **Atenção:** o CLAUDE.md global manda usar `mcp__github-official__*` e deixa o `gh` como último recurso. O `omhl check` é binário determinístico (não o agente) e não tem acesso a MCP, então `gh` ali parece aceitável. Decisão do usuário.
- cargo-deny 0.20.2 e cargo-audit 0.22.2: opcionais.

## Lista final
- **Usar:** clap, serde, serde_json, schemars, thiserror, anyhow, sha2+hmac, serde_json_canonicalizer, fs4, atomic-write-file, process-wrap, assert_cmd, insta, proptest, tempfile, trycmd, cargo-nextest, cargo-dist, cargo-binstall, plugin nativo (`hooks.json`, `bin/`, `claude plugin tag`, `dependencies`), `onFailure:"block"`, git rerere + range-diff, just, mise, `gh api` por SHA e GraphQL.
- **Evitar:** jsonschema (por ora), miette, blake3, rmcp, act como prova de CI, git subtree, StGit/quilt para o fork, `claude-hooks`/`agent-config`/`agentgear`, copiar do `destructive_command_guard` sem ler a licença, dois gerenciadores de git hooks no mesmo repo.
- **Já resolvido por terceiros:** fail-closed de hook (`onFailure:"block"`, exceto Stop); registro, versionamento e atualização de hooks (plugin); distribuição do binário (cargo-dist + cargo-binstall); hash canônico RFC 8785 nos dois lados (falta teste de vetores); schema do contrato (schemars gera, ajv já no Hexlog valida); bloqueio de edição de settings pelo agente (ConfigChange, que não cobre o `hooks.json` do plugin).
- **Escrever do zero:** tipos de hook em Rust, máquina de estados do Stop e conferência do recibo, token preso ao hash.

## Não verificado
Equivalência de saída entre `serde_json_canonicalizer` e `canonicalize` (npm); formato do `tool_response` de tool MCP no PostToolUse; resolução de dois `updatedInput` conflitantes; se o modelo enxerga o `updatedInput` injetado; namespace de comandos de plugin; distribuição do binário via plugin `npm` com pacotes por plataforma.

## Próximo passo sugerido pelo agente
Um hook de sonda que grave os payloads reais de PreToolUse e PostToolUse de uma tool do Hexlog. Fecha os quatro primeiros itens não verificados e gera as fixtures do `omhl`.
