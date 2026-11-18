# Evidência do canal `skills:` e do papel visto pelos hooks

Medição de 2026-11-18, Claude Code `2.1.327`, tudo em plugins descartáveis e sandbox. Esta página resume o que foi medido e o que a medição sustenta; os dados redigidos estão em [spike1/](spike1/) e [g1/](g1/). Nenhum teste lê este arquivo: os valores que dependem dele (`SKILLS_SPELLING` e `CHANNEL`, constantes do `role_view`) ficam no código, com esta página citada ao lado.

## O que se queria saber

1. O campo `skills:` do agente pré-carrega a vista do papel (`role-<papel>`) no subagente, e em qual grafia?
2. Qual `agent_type` o hook `PreToolUse` vê numa chamada ao `Agent` com e sem `name`? O hook `agent-role` precisa de `oh-my-hexflow:<papel>`; com outro valor ele não aplica a matriz `omhl-allow` e o `omhl` grava o registro como `claude`.

## Spike 1: leitura corrigida

Plugin `spikeplug` com dois agentes (`model: haiku`, `omitClaudeMd: true`, sem `disallowedTools`), a skill canário `role-probe` e dois hooks (`SubagentStart` e `PreToolUse` com matcher só `Bash`) que gravam o stdin inteiro. Quatro execuções `claude -p`, sem time, `--no-session-persistence`. Comando e script em [spike1/run.sh](spike1/run.sh); o `run.sh` escrevia `.hooks.log`, e os logs foram guardados como `.hooks.tsv` aqui.

| Execução | `skills:` | Chamada | Canário da skill | `agent_type` nos dois hooks | `requestShape` |
|---|---|---|---|---|---|
| r1 | `[role-probe]` | sem `name` | sim | `spikeplug:probe` | `background` |
| r2 | `[role-probe]` | `name: probe-named` | sim | `spikeplug:probe` | `background` |
| r3 | `[spikeplug:role-probe]` | sem `name` | sim | `spikeplug:probe-q` | `background` |
| r4 | `[spikeplug:role-probe]` | `name: probe-q-named` | sim | `spikeplug:probe-q` | `foreground` |

Todas as execuções tiveram `requestNonInteractive: true` e nenhum `taskKind`. O campo `requestNonInteractive` não distingue headless de interativo (vale `true` nos dois, inclusive nos `meta.json` da parte B); o critério usado é o do roteiro (`claude` com ou sem `-p`) e o `taskKind`. Arquivos: [out/](spike1/out/), [meta/](spike1/meta/), [VERSION.txt](spike1/VERSION.txt).

**Sustenta:** em headless e sem time, o canário chegou nas quatro respostas e o `agent_type` dos dois hooks foi `<plugin>:<agente>`, com e sem `name`.

**Não prova:** (i) pré-carga contra carga sob demanda: os agentes tinham a tool `Skill`, o `PreToolUse` só registrava `Bash` e não houve controle negativo; (ii) o caminho interativo com time (`in_process_teammate`), onde se esperava que o `agent_type` fosse o nome gerado. O G1 existe para esses dois pontos, e onde os dois divergem vale o G1.

## G1: a medição que separa pré-carga de carga

Plugin `omhlprobe` com cinco agentes `haiku`, a skill `role-probe` com o canário `SKILLCANARY-7a4d7a18` e hooks `SubagentStart` e `PreToolUse` sem matcher que só gravam. Quatro agentes bloqueiam `Skill`, `Read`, `Grep`, `Glob` e `WebFetch` (sem essas tools, o canário só chega por pré-carga); `probe-qual-skill` deixa a `Skill` liberada. Controles negativos: `probe-none` (sem `skills:`) e `probe-missing` (skill inexistente). Parte A: seis execuções headless, sem `--no-session-persistence`. Parte B: sessão interativa, seis chamadas ao `Agent`, uma por vez. Plugin da sonda em [g1/probe-plugin/](g1/probe-plugin/); roteiros em [g1/run-a.sh](g1/run-a.sh) e [g1/partB-prompts.md](g1/partB-prompts.md); leitura original em [g1/RESULT-g1.md](g1/RESULT-g1.md).

| Chamada | Agente, modo | Canário | `agent_type` no `PreToolUse` | `taskKind` | `customAgentType` | `requestShape` |
|---|---|---|---|---|---|---|
| A1 | `probe-bare`, sem `name`, headless | sim | `omhlprobe:probe-bare` | - | - | `background` |
| A2 | `probe-qual`, sem `name`, headless | sim | `omhlprobe:probe-qual` | - | - | `foreground` |
| A3 | `probe-none`, sem `name`, headless (controle 1) | `NONE` | `omhlprobe:probe-none` | - | - | `background` |
| A4 | `probe-missing`, sem `name`, headless (controle 2) | `NONE` | `omhlprobe:probe-missing` | - | - | `background` |
| A5 | `probe-qual`, `name: g1-a-named`, headless | sim | `omhlprobe:probe-qual` | - | - | `foreground` |
| A6 | `probe-qual-skill` (`Skill` liberada), sem `name`, headless | sim | `omhlprobe:probe-qual-skill` | - | - | `background` |
| B1 | `probe-qual`, sem `name`, interativa | sim | `omhlprobe:probe-qual` | - | - | `background` |
| B2 | `probe-qual`, `name: g1-probe-named`, interativa | `NONE` | `g1-probe-named` | `in_process_teammate` | `omhlprobe:probe-qual` | `background` |
| B3a | `probe-none`, sem `name`, interativa (controle de B1) | `NONE` | `omhlprobe:probe-none` | - | - | `background` |
| B3b | `probe-none`, `name: g1-none-named`, interativa (controle de B2) | `NONE` | `g1-none-named` | `in_process_teammate` | `omhlprobe:probe-none` | `background` |
| B4 | `oh-my-hexflow:planner`, sem `name`, interativa | n/a | `oh-my-hexflow:planner` | - | - | `background` |
| B5 | `oh-my-hexflow:planner`, `name: g1-planner-named`, interativa | n/a | `g1-planner-named` | `in_process_teammate` | `oh-my-hexflow:planner` | `background` |

Nas linhas B, a coluna Canário vem do marcador de pré-carga nas transcrições ([g1/transcripts/b*.summary.txt](g1/transcripts/)) e de [g1/RESULT-g1.md](g1/RESULT-g1.md); a resposta literal do subagente não foi guardada. Fontes por linha: [g1/out/](g1/out/) (respostas, logs de hooks e [b.analysis.txt](g1/out/b.analysis.txt)), [g1/meta/](g1/meta/) e [g1/transcripts/](g1/transcripts/).

**Marcador de pré-carga.** Nas transcrições, a pré-carga aparece como um registro `user` com `<command-message>role-probe` (na grafia qualificada, `<command-message>omhlprobe:role-probe`) **antes** do primeiro `tool_use`. Os `*.summary.txt` numeram os registros a partir de 0: o marcador é o registro 3 em A1, A2, A5, A6 e B1; o primeiro `tool_use` vem no registro 17 (A1, A2, A5) ou 18 (A6, B1). A3, A4, B2, B3a, B3b, B4 e B5 não têm marcador. Nenhum subagente chamou `Skill`, `Read`, `Grep` ou `Glob`; só rodaram `echo g1` no `Bash` e, na parte B, as tools de devolução do harness (`SubagentHandback`, `SendMessage`). No log da parte B (extrato da sessão principal), a sessão não chamou `Skill` e `prompt_has_canary` é falso nas seis chamadas; na parte A a linha da sessão principal não entra no repositório (redação), e essa conferência foi feita sobre a pasta crua.

## Leitura pelo critério de cinco condições

- **Sem `name`:** pré-carga provada nas duas grafias em headless (A1 bare, A2 qualificada, A6 qualificada com a `Skill` liberada, controles A3 e A4) e na qualificada em interativo (B1, controle B3a).
- **Com `name`:** em interativo, o canário não chega (B2; o controle B3b dá `NONE`, então vale). Em headless com `name` a skill chegou (A5), sem `in_process_teammate`: a perda é do caminho de teammate da sessão interativa.
- **`agent_type`:** com `name`, `PreToolUse` e `SubagentStart` veem o nome gerado (B2, B5); sem `name`, veem `<plugin>:<agente>` (B1, B4). O papel real só aparece no `meta.json` do subagente (`customAgentType`).
- **Skill inexistente em `skills:`** é ignorada em silêncio (A4): sem erro de criação e sem canário.

## A linha que valeu

Pré-carga provada sem `name` (B1 com o controle B3a), `agent_type` = nome gerado com `name` (B5) e `oh-my-hexflow:planner` sem `name` (B4).

- **Canal:** `CHANNEL` = `skills:` (não o plano B, que poria a vista no corpo do agente).
- **Grafia:** `SKILLS_SPELLING` = qualificada, `oh-my-hexflow:role-<papel>`; provada em interativo (B1).
- **Regra de uso:** as skills de workflow chamam `oh-my-hexflow:*` **sem** `name`. Com `name`, o agente roda sem a matriz `omhl-allow` e sem a vista pré-carregada, e o que gravar deve sair como `claude` (consequência do hook `agent-role`, ADR 0008; o G1 não gravou registros).
- **Fix do hook:** um `PreToolUse` do plugin nega `name` nas chamadas a `oh-my-hexflow:*`, em work item `fix` próprio.

## Resposta do dono à pergunta 4

Pergunta: como o papel chega ao hook `agent-role`, dado que `name` troca o `agent_type` pelo nome gerado. Resposta literal em 2026-11-18, opção (a'), chamar sem `name` e impor isso por máquina:

> a' -> para build frio para instalação de prod é mais tranquilo

A condição da opção (B1 provada e B4 com o prefixo `oh-my-hexflow:`) **se confirmou** nesta medição. A autorização do build frio vale só para o `cargo install` de produção do `omhl` desse fix; a sessão avisa e espera a ordem na hora.

## Desvios do procedimento

- O agente da parte B foi iniciado sem `SPIKE_LOG`, e o script de log passou a gravar por padrão em `out/b.hooks.tsv`: a parte B tem um log único (36 linhas: 6 da sessão principal, 18 de subagente e 12 `META`), não um por chamada. Em `g1/out/b.hooks.tsv` a sessão principal aparece só como extrato (`tool_name`, `subagent_type`, `name`, `prompt_has_canary`).
- A parte B foi conduzida pela sessão (Herdr), não pelo dono.
- `redact.sh` e os `transcripts/*.summary.txt` foram escritos depois do G1, para esta cópia: [g1/redact.sh](g1/redact.sh) reproduz a redação a partir da pasta crua, que não entra no repositório. Nos resumos, os registros de prompt de sistema, anexos e sistema aparecem como `omitido` (nem o tipo entra).
- Redação: `session_id`, `transcript_path`, `cwd`, `scratchpad_dir` e `prompt_id` viram `<redigido>`; nos scripts e no roteiro, o caminho do repositório vira `<repo>` e o do scratchpad, `<scratchpad>`. Nenhuma transcrição crua nem `*.err.txt` (só o aviso de login) entra.

## O que não foi provado

- A grafia bare em sessão interativa (a parte B só usou a qualificada).
- Colisão de nomes de skill entre plugins no `skills:` (existe um só `role-probe`).
- Pré-carga em interativo **com a tool `Skill` disponível**, o caso dos agentes reais: B1 usa `probe-qual`, que a bloqueia. A6 (headless, `Skill` liberada) pré-carregou sem chamar a `Skill`; o caso interativo fica para a conferência do piloto.
- Outra versão do Claude Code.
- Pré-carga com `name`: o G1 mostra que não acontece (B2), e por isso o desenho não a usa.
