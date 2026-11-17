# O que fazer quando o check ou o hook recusa

Quando consultar: o `omhl check run` ou um hook de git (`pre-commit`, `pre-push`) saiu com erro e você precisa saber o que tentar. Os erros de cada comando estão na página dele; aqui ficam as recusas que impedem o veredito.

## Falha sem veredito

Um erro sem veredito não é um `failed`: o `omhl` não consegue dizer se o check passou e não grava attestation. Cada item de `details` nomeia o check (`path`), o motivo (`code`) e, na maioria dos casos, um `context` com `kind` e `hint`. Nada do `context` vai para o banco.

| `code` do detalhe | O que fazer |
|---|---|
| `spawn-failed` | O binário do `run` não existe no `PATH` do git. Instale-o ou ponha a pasta dele no `PATH` dos hooks, e repita o mesmo comando |
| `outcome-immutable` | O mesmo snapshot já tem outro desfecho. Mude a árvore (um commit novo) para um veredito novo |
| `command-changed-tree`, `command-changed-worktree` | O check alterou a árvore. Mantenha-o somente leitura; ponha o que ele gera no `.gitignore` ou restaure os rastreados com `git checkout -- <caminho>` |
| `spec-stale` | O TOML do check diverge do banco. Corrija o arquivo, rode `omhl load` e repita |
| `spec-invalid` | O `data` do check no banco não decodifica. Corrija o TOML e rode `omhl load` |
| `snapshot-tree-mismatch`, `snapshot-dirty` | O índice mudou durante o hook, ou a árvore está suja. Não faça stage durante o hook e rode de novo |
| `work-item-closed` | O work item foi fechado durante a execução. Confira com `omhl get <work-item>` |
| `probe-failed` | O `omhl` não leu a árvore ou o `HEAD`. Rode `git status` e corrija o estado do repositório |
| `attestation-not-recorded` | O banco ficou travado depois de o comando rodar. O veredito foi calculado e não gravado: repita o mesmo comando |

Quando só há checks bloqueantes reprovados, o erro é `checks-failed`: leia a causa com `omhl byproduct` ([byproduct.md](byproduct.md)). Um problema sem veredito tem precedência sobre `checks-failed`.

## Branch e work item

O `omhl check run` descobre o work item pelo nome da branch `tipo/wi-<id>__<slug>`. Na `main` ele responde `branch-protected` e não roda nada; fora do padrão, `branch-name-invalid`. Na `develop` ele roda sem gravar nada, e o `before-commit` pede a marca da `develop`.

## Push

O `pre-push` só atesta a branch do checkout. Um `git push` de outra ref, sem trocar de checkout, é barrado com `push-ref-not-checked-out`. Push com destino `main` é barrado com `branch-protected`.

## Bypass é do dono

Nunca use `--no-verify` nem `--force` do `omhl hooks install` por conta própria: contornar um hook é decisão do dono, não sua.

## Prazo do check

Cada check tem um `timeout_secs` no TOML. Um check que estoura o prazo grava `failed`, imutável para aquela árvore. Reajustar o prazo é decisão do dono: o valor se mede com o `target/` limpo (rodada mais lenta vezes 1,3, piso de 10 segundos) e se grava no TOML, seguido de `omhl load`.
