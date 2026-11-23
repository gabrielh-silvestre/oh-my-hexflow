# omhl check sweep

## Quem usa, em qual etapa e com qual filtro

Nenhuma etapa do ciclo a roda.

A varredura global é do dono e só roda com a aprovação do humano: `SKILL.md` da `review`, seção Regras.

## Quando usar

Quando sobrou processo de um check cujo `omhl` já saiu (Ctrl+C, `kill`, queda do terminal). Antes de rodar, peça a aprovação do humano: o comando vale para **qualquer raiz de projeto da máquina**, do mesmo usuário, e pode encerrar um servidor (`sccache`, daemon do Gradle) que outra sessão ainda usa. Para o caso comum, rode o `omhl check run`: ele já varre, em silêncio, os órfãos do próprio projeto no começo de cada execução. Este comando faz a varredura sob demanda. Só roda no Linux.

## Entradas

- Sem argumentos posicionais.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco; o `--project-dir` precisa existir.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. O padrão é `agent`; `human` mostra os mesmos fatos em blocos; `json` é a saída para máquinas.

## Saída

Texto no stdout, saída 0, também quando não há alvo: `✔ check sweep · N swept · N unreadable` e uma linha `- <comm> · pid <pid> · owner <pid>` por processo encerrado. Quando a leitura de `/proc` estoura o tempo (no máximo 1 segundo), o cabeçalho vira `⚠ check sweep · N swept · N unreadable · scan timed out` e fecha com `Next:` mandando repetir o comando, porque a varredura desistiu antes de ler tudo e o `swept` vazio não prova nada. Com `--format json` é uma linha JSON: `swept` (uma lista; cada item tem `pid`, `comm` e `owner_pid`), `unreadable` (quantos processos não puderam ser lidos) e `scan_timed_out`. Por varrer a máquina inteira, **`swept`, `unreadable` e `scan_timed_out` (no texto, as contagens e os pids) não são determinísticos**: dependem dos processos vivos naquele momento. Não leia um `swept` vazio como prova de que nada vazou em outro projeto, e não conte com o mesmo número em duas chamadas. Quando acha alvo, a varredura encerra o processo (`TERM`, 2 segundos de carência, `KILL`) e acrescenta uma linha a `$XDG_DATA_HOME/omhl/sweep.jsonl`. O arquivo tem teto de 1 MiB: acima disso ele passa a `sweep.jsonl.1` (uma só geração antiga, a anterior é substituída) e um `sweep.jsonl` novo recebe a linha. Duas rotações simultâneas podem perder uma geração.

## Erros

Um erro sai no stderr, com saída 1, em texto (`✘ código · mensagem`; com `--format json`, numa linha JSON). Uma opção desconhecida ou um argumento que falta é erro de uso do clap, com saída 2 e sem código. Os erros comuns a todos os comandos estão em [errors.md](../../directives-flow/references/errors.md).

- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Varredura sem alvo. Em outra máquina, ou com outros processos vivos, `swept`, `unreadable` e `scan_timed_out` mudam:

```omhl-example fixture=checks
$ omhl check sweep
✔ check sweep · 0 swept · 0 unreadable
```

A mesma varredura para máquina:

```omhl-example fixture=checks
$ omhl check sweep --format json
{"scan_timed_out":false,"swept":[],"unreadable":0}
```
