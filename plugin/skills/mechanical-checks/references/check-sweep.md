# omhl check sweep

## Quando usar

Quando sobrou processo de um check cujo `omhl` já saiu (Ctrl+C, `kill`, queda do terminal). O `omhl check run` já varre, em silêncio, os processos do próprio projeto no começo de cada execução; este comando faz a varredura sob demanda e vale para **qualquer raiz de projeto da máquina**, do mesmo usuário. Só roda no Linux.

## Entradas

- Sem argumentos posicionais.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco; o `--project-dir` precisa existir.

## Saída

Uma linha JSON no stdout, saída 0, também quando não há alvo: `swept` (uma lista; cada item tem `pid`, `comm` e `owner_pid`), `unreadable` (quantos processos não puderam ser lidos) e `scan_timed_out` (a leitura de `/proc` espera no máximo 1 segundo). Por varrer a máquina inteira, **`swept` e `unreadable` não são determinísticos**: dependem dos processos vivos naquele momento. Não leia um `swept` vazio como prova de que nada vazou em outro projeto, e não conte com o mesmo número em duas chamadas. Quando acha alvo, a varredura encerra o processo (`TERM`, 2 segundos de carência, `KILL`) e acrescenta uma linha a `$XDG_DATA_HOME/omhl/sweep.jsonl`. O exemplo abaixo mostra o caso vazio; o teste normaliza `swept` e `unreadable`.

## Erros

Um erro sai no stderr, numa linha JSON, com saída 1. Uma opção desconhecida ou um argumento que falta é erro de uso do clap, com saída 2 e sem código.

- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Varredura sem alvo. Em outra máquina, ou com outros processos vivos, `swept` e `unreadable` mudam:

```omhl-example fixture=checks
$ omhl check sweep
{"scan_timed_out":false,"swept":[],"unreadable":0}
```

Recusa: `--project-dir` que não existe:

```omhl-example fixture=checks
$ omhl check sweep --project-dir /nonexistent
2> {"error":{"code":"invalid-project-dir","details":[],"message":"the project directory does not exist"}}
# exit 1
```
