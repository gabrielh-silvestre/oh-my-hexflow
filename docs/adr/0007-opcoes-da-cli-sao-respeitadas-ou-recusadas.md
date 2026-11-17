---
status: accepted
---

# Opções da CLI são respeitadas ou recusadas

## Contexto

Várias opções da CLI mudavam o pedido do agente sem dizer nada. O `byproduct --tail 501` devolvia 500 linhas, saída 0. O `list --type premises` (plural), o `list --where blokcing=true` e o `list --id-prefix premise` devolviam `{"items":[]}`, saída 0, e o agente lia o `[]` como "não há nada" quando a causa era um filtro que nunca casa. O `--project-dir` inexistente subia pelos ancestrais, achava o projeto de um diretório acima e respondia por ele: num `append`, o registro iria para o tenant errado, num banco append-only. O `report answers --work-item` de um work item que não existe devolvia `{"gaps":[]}`. Cinco recusas que já existiam diziam só "unknown value", sem a regra.

Quem consome a CLI é um agente, e o agente confia no que pediu.

## Decisão

Uma opção que o agente passa num comando da CLI é respeitada. Se ela viola uma regra, a CLI a recusa com um erro que diz a regra, em vez de cortar, ajustar ou ignorar em silêncio. O erro é `invalid-arguments` com ponteiro, ou um código de domínio que já existe (`work-item-not-found`, `invalid-project-dir`). A mensagem diz a regra e nunca repete o valor recebido. A exceção é o valor que o clap nem consegue ler (`--tail abc`, `--tail -1`, `--limit 99999999999`, `--limit -1`, `--min -1`, `--after-seq abc`). Esse sai pelo erro de uso do clap: texto no stderr e saída 2, como qualquer erro de sintaxe (`--bogus`). A saída 2 é escolha do `report_usage`, e a mensagem do clap repete o valor recusado. Nada é cortado nem ajustado em silêncio, então a B3 vale.

As recusas moram em `Engine` (a validação do `list`, do `byproduct`, do `attest` e do `report`) e em `run_action` (a checagem do `--project-dir`), então valem para qualquer porta do `omhl`. A checagem do `--project-dir` fica fora de `Runtime::discover`, que o hook também chama: a T1 segue valendo para o hook.

Esta decisão é uma exceção documentada à leitura geral da regra de que a saída da CLI não muda (T1). Ela tem a conta fechada:

- **Oito recusas novas.**
  - I1: `byproduct --tail` acima de 500 recusa (`/tail`, "tail must be between 1 and 500"); antes devolvia as últimas 500 linhas.
  - I2: `report answers --work-item` de um work item que não existe recusa com `work-item-not-found`.
  - I4: com `--type`, o `--where` recusa campo que o tipo não declara, campo que não é de texto e valor fora do `enum` do campo.
  - I5: o mesmo campo do `--where` repetido com valores diferentes recusa; o par idêntico repetido segue aceito.
  - I6: `--type` que não existe no catálogo recusa.
  - I9: `--project-dir` que não existe recusa com `invalid-project-dir` (inclui o `gate-worktree`, que antes saía 0, mudo).
  - I10: sem `--type`, o `--where` recusa quando nenhum tipo candidato o satisfaz inteiro (a regra de tipos candidatos).
  - I12: `--id-prefix` cujo primeiro segmento não é namespace de tipo, ou não é o do `--type`, recusa.
- **Duas recusas que mudam.**
  - I3: o `--id-prefix` passa a recusar segmento inválido (vazio, começando por `-`, acima de 63 caracteres) e prefixo acima de 200 caracteres; antes só conferia alfabeto e tamanho total.
  - I11: o `--limit` acima do teto da página (`Limits::default().page_max`, 200) e o `--limit 0` passam a ter um erro só, `invalid-arguments` em `/limit`; o acima de 200 tinha outro código e outro ponteiro.
- **Cinco mensagens que passam a dizer a regra (I7).** `--limit`, `--tail`, `--where` malformado, `--id-prefix` e `--outcome` recusavam com "unknown value". O código e o ponteiro não mudam.
- **Um ponteiro que muda.** O nome de campo inválido no `--where` (por exemplo `Bad Field=x`) já era recusado; o ponteiro passa de `/where/<campo>` para `/where/<índice>` do par, igual ao das demais recusas do `--where`. O código e a saída 1 não mudam.
- **Um campo aditivo (I8).** O `byproduct` devolve `truncated_lines`, só quando é maior que zero: quantas das linhas devolvidas passaram de 2000 caracteres e foram cortadas. O corte da linha é limite de transporte; o campo o torna visível.

O `--where` e o `--id-prefix` do `list` seguem uma regra única: o filtro vale se algum **tipo candidato** o satisfaz inteiro. Com `--type`, o candidato é só esse tipo. Sem `--type`, são os tipos que o namespace do `--id-prefix` admite, ou os nove do catálogo se não há prefixo. Cada `--where` reduz o conjunto aos candidatos que declaram o campo como texto e aceitam o valor; o filtro é recusado quando o conjunto esvazia. O que sobra fora da regra é resposta vazia verdadeira, que depende do estado do banco (um `work_item` sem nodes, um `--after-seq` além da cabeça).

**Campo de texto.** Um campo é de texto quando o schema dele tem `type: "string"`, ou quando não tem `type` e todo valor do `enum` é string (é assim que `status`, `kind`, `resolution`, `concern`, `outcome` e outros entram). `boolean`, `integer`, `array` e `object` não são.

## Drivers

- B3: a opção que o agente passa é respeitada ou o agente é avisado; o dono a confirmou em 2026-11-16 e disse que a lista de casos podia não ser a única.
- T1: a saída da CLI não muda sem decisão. A exceção está aqui, com a conta fechada.
- T3: a regra mora em `app` (`Engine` e `run_action`) e vale para qualquer porta, sem lógica no adapter da CLI além da sintaxe `FIELD=VALUE`.
- Custo de execução: o agente que lê `[]` como "nada existe" gasta uma volta inteira antes de achar o filtro errado.

## Alternativas consideradas

- **Manter o corte de 500 e documentá-lo.** Foi a decisão anterior; o dono a reverteu. Documentar o corte não avisa o agente que o pediu.
- **`value_parser` do clap.** Descartada: levaria o teto de 500 do `--tail` e o de 200 do `--limit` para o adapter, contra a T3. O erro de leitura do valor já é erro de sintaxe, com saída 2 fixada em `tests/cli.rs`; levá-lo a JSON com saída 1 mudaria a saída de todo erro de sintaxe, fora da conta fechada da T1.
- **Corrigir só o `byproduct`.** Descartada: o dono pediu todos os comandos.
- **Aviso em `warnings[]` em vez de recusa.** Há precedente no `hooks`. Descartada: um aviso ao lado de `[]` convida o agente a confiar no `[]`.

## Avaliados e mantidos

Cortes e silêncios que a revisão examinou e que ficam como estão. Os outros casos avaliados estão nas recusas acima.

| Onde | Comportamento | Por que fica |
|---|---|---|
| `--limit` omitido (`Engine::list`, padrão 50) | Devolve a primeira página de 50 | Omitir a opção não é pedir um valor; `next_after_seq` segue devolvido. |
| `get` de id inexistente ou fora de namespace | `{"node":null}` | É o contrato do comando; o I12 vale para o prefixo do `list`, não para o `get`. |
| Cortes que já avisam (`omitted` e `too many errors to list` em `app/error.rs`, `truncated` em `app/check_run.rs`, `EVIDENCE_CAP`, `TRUNCATION_MARKER`) | A lista ou o texto sai cortado, com o aviso | O agente é avisado do corte; a catraca só vê `src/app`. |
| `output_tail` do `check run` (20 linhas de 500 caracteres) | Resumo da falha cortado | Resumo da falha, não opção; a saída inteira fica no `byproduct`. |
| `--pre-push` sem `--event before-pr` | Recusa | Já recusa. |
| `hooks --force` | Sobrescreve os hooks | Flag documentada; o efeito é o pedido. |
| `append` com chave JSON duplicada | A última chave vale | `serde_json` mantém a última; detectar exige parser próprio, custo alto, não proposto. |

## Por que esta

O agente confia no que pediu. O que volta é o que ele pediu ou um erro que diz a regra, nunca um parente próximo do pedido.

## Consequências

- Respostas vazias, cortadas ou desviadas (I1 a I6, I9, I10, I12) viram erro, e o segundo PR muda a saída de comandos que o primeiro manteve. Quem chamava `--tail` acima de 500 passa a receber saída 1; o `gate-worktree` com `--project-dir` inexistente passa de saída 0 mudo para saída 1 (o silêncio segue para o diretório que existe e não tem marcador).
- O `pre-switch` do `wt` não passa `--project-dir`, então o hook do `wt` não muda.
- A semântica do `--id-prefix` (casa por segmento inteiro: `premises.cli1` não casa `premises.cli1-b1`; começa por um namespace de tipo) e a definição de campo de texto do `--where` passam a estar escritas em [directives-flow.md](../directives-flow.md).
- **A validação do filtro depende do catálogo estático** (`graphs`): se um dia os tipos vierem por extensão do projeto, I4, I6, I10 e I12 passam a depender do tenant e a regra muda de lugar.
- A regra de tipos candidatos tem consequência prática: como um tipo precisa declarar todos os campos do `--where` como texto e nenhum tipo tem mais de seis campos de texto, mais de seis nomes de campo distintos nunca passam.
- O `hexlog` tem a sua própria porta e segue silencioso para os mesmos filtros: a regra vale para as portas do `omhl`.
- Um teste de arquitetura, a catraca, vigia só o corte numérico de `src/app` (`.min(`, `.max(`, `.clamp(`, `.take(`, `saturating_`): a lista fechada hoje soma 9 ocorrências, e uma nova reprova até ser justificada. Ele não vê validação ausente.

## Follow-ups

- Converter o valor do `--where` pelo tipo do campo (`blocking=true` como booleano); o núcleo já tem `Scalar::{Str,Int,Bool}`.
- Avisar quando o `--db` cria o banco.
- O `--min` do `report recurrences` só ganha regra se o dono criar uma.
- Recusar `--db` e `--project-dir` no grupo `hook`, se o dono pedir.
- A catraca vigia só o corte numérico de `src/app`; cortes em outros módulos e validação ausente ficam fora dela.
- O próximo comando novo nasce sob esta regra.
- Casos achados depois do merge vão para um work item novo.
