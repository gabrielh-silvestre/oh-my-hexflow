# omhl check sweep

## Quando usar

Quando sobrou processo de um check cujo `omhl` já saiu (Ctrl+C, `kill`, queda do terminal). Antes de rodar, peça a aprovação do humano: o comando vale para **qualquer raiz de projeto da máquina**, do mesmo usuário, e pode encerrar um servidor (`sccache`, daemon do Gradle) que outra sessão ainda usa. Para o caso comum, rode o `omhl check run`: ele já varre, em silêncio, os órfãos do próprio projeto no começo de cada execução. Este comando faz a varredura sob demanda. Só roda no Linux.

## Entradas

- Sem argumentos posicionais.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco; o `--project-dir` precisa existir.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. Por ora este comando só imprime `json`, o padrão; `agent` e `human` saem com erro de uso (saída 2).

## Saída

Uma linha JSON no stdout, saída 0, também quando não há alvo: `swept` (uma lista; cada item tem `pid`, `comm` e `owner_pid`), `unreadable` (quantos processos não puderam ser lidos) e `scan_timed_out` (a leitura de `/proc` espera no máximo 1 segundo; com `scan_timed_out: true` a varredura desistiu antes de ler tudo, então o `swept` vazio não prova nada: repita o comando). Por varrer a máquina inteira, **`swept`, `unreadable` e `scan_timed_out` não são determinísticos**: dependem dos processos vivos naquele momento. Não leia um `swept` vazio como prova de que nada vazou em outro projeto, e não conte com o mesmo número em duas chamadas. Quando acha alvo, a varredura encerra o processo (`TERM`, 2 segundos de carência, `KILL`) e acrescenta uma linha a `$XDG_DATA_HOME/omhl/sweep.jsonl`.

## Erros

Um erro sai no stderr, numa linha JSON, com saída 1. Uma opção desconhecida ou um argumento que falta é erro de uso do clap, com saída 2 e sem código. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Varredura sem alvo. Em outra máquina, ou com outros processos vivos, `swept`, `unreadable` e `scan_timed_out` mudam:

```omhl-example fixture=checks
$ omhl check sweep
{"scan_timed_out":false,"swept":[],"unreadable":0}
```
