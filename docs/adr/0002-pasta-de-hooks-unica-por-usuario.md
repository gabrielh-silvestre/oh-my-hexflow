---
status: accepted
---

# Pasta de hooks do git única por usuário

## Contexto

O `omhl hooks install` precisa de um lugar para os scripts que chamam o `omhl check run` nos eventos do git. O binário `plugin/bin/omhl` fica fora do `PATH`, e o `core.hooksPath` é config local do clone: não viaja com ele. Um caminho relativo some numa worktree cujo checkout não tem a pasta, e o commit passa sem hook e sem aviso. O dono decidiu em 2026-11-10, entre quatro opções:

| Opção | Cobre | Limite |
|---|---|---|
| `<git-common-dir>/omhl/hooks/` por repo, `core.hooksPath` absoluto | todas as worktrees do repo | não viaja com o clone; atualizar o `omhl` obriga a reinstalar em cada repo |
| pasta única de usuário, `core.hooksPath` absoluto | uma só atualização para todos os repos; combina com o banco | instalar num repo reescreve os scripts dos outros |
| `.omhl/hooks/` versionado | viaja com o clone | o caminho absoluto do binário quebra em outra máquina; o relativo some em worktree |
| `.git/hooks` padrão, sem `core.hooksPath` | simples | não é uma pasta que o `omhl` controle |

## Decisão

Os scripts ficam em `$XDG_DATA_HOME/omhl/hooks/` (ou `~/.local/share/omhl/hooks/`), a mesma regra do banco, e o `core.hooksPath` local do clone recebe esse caminho absoluto. O `omhl hooks install` confere que o git resolve o hook por essa pasta antes de dar a instalação por feita.

## Consequências

- Uma atualização do `omhl` vale para todos os repos do usuário, e a pasta anda junto com o banco.
- Os scripts levam o caminho absoluto do binário, e todo clone compartilha os mesmos scripts. Por isso a instalação avisa (`binary-in-worktree`, `binary-in-build-dir`) quando o binário está dentro de uma worktree ligada ou de `target/` (a worktree principal não avisa): um `wt remove` apagaria o binário, e o hook de todos os clones sairia com 1.
- Cada clone roda `omhl hooks install` uma vez. Um clone sem a instalação não tem hook, e o `gate-pr` cobra depois (ver `docs/mechanical-gates.md`).
- Instalar num repo reescreve os scripts dos outros. A instalação recusa um `core.hooksPath` que aponta para outra pasta (`hooks-path-conflict`) e hooks existentes em `.git/hooks` (`hooks-existing`), salvo `--force`.
