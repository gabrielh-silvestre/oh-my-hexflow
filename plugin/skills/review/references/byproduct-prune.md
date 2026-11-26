# omhl byproduct prune

## Who uses it, at which stage and with which filter

| Role | Stage | Command |
|---|---|---|
| session | review rules | `omhl byproduct prune` |

A matriz dos papéis não o tem: nenhum agente de papel o roda, e o hook `agent-role` o nega com `command-outside-matrix`. Só a sessão principal e os agentes sem papel portado o usam.

## Quando usar

Para apagar já as saídas guardadas que venceram a retenção, sem esperar o próximo `omhl check run`. O `check run` já faz a mesma poda sozinho, em silêncio, no começo de cada execução (qualquer evento), no máximo **uma vez por dia por (banco, projeto)**: um carimbo `.pruned-at` na pasta do (banco, projeto) segura as demais. Este comando ignora o carimbo e o renova.

A regra é a mesma nos dois: apaga o arquivo de saída (e o temporário `.tmp-*` que sobrou de uma gravação interrompida) com mais de **7 dias** de data de modificação, **exceto** o de uma attestation viva. Viva é a attestation de maior `seq` de cada par (check, work item) do projeto: a saída do veredito vigente nunca some, qualquer que seja a idade, então `omhl byproduct <check> <work-item>` não devolve `byproduct-missing` por causa da retenção. A saída de uma árvore anterior do mesmo par some depois de 7 dias. O gate de pull request lê só o desfecho e não muda; a saída do veredito que ele aceitou é que pode já ter sumido.

O que a poda enxerga é só a pasta `blobs/by-db/<arquivo do banco>/<projeto>` ao lado do banco: outro projeto ou outro banco na mesma pasta nunca é tocado. Os arquivos do layout antigo (`blobs/<xx>/<hash>`, de antes da pasta por banco e projeto) continuam legíveis, mas **nunca são podados**: a higienização deles é manual, uma vez, depois da entrega.

## Entradas

- Sem argumentos posicionais e sem opções próprias: a retenção é fixa, 7 dias.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco; o `--project-dir` precisa existir.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. O padrão é `agent`; `human` mostra os mesmos fatos em blocos; `json` é a saída para máquinas.

## Saída

Texto no stdout, saída 0, também quando não há nada a apagar: `✔ byproduct prune · N removed · N kept live · N bytes freed`. Com `--format json` é uma linha JSON: `removed` (arquivos apagados), `kept_live` (arquivos com mais de 7 dias mantidos por serem de attestation viva) e `freed_bytes` (o tamanho dos apagados). Os números dependem do que está em disco naquele momento.

## Erros

Um erro sai no stderr, com saída 1, em texto (`✘ código · mensagem`; com `--format json`, numa linha JSON). Uma opção desconhecida ou um argumento que falta é erro de uso do clap, com saída 2 e sem código. Os erros comuns a todos os comandos estão em [errors.md](../../directives-flow/references/errors.md).

- `byproduct-prune-failed` (saída 1): o banco ou a pasta das saídas não pôde ser lido, ou um arquivo vencido não pôde ser apagado (sem permissão de escrita na pasta, por exemplo). Confira as permissões de `blobs/by-db/` ao lado do banco e rode o comando de novo. O `omhl check run` não mostra esse erro: a poda dele não tem canal de erro, e é este comando que o revela.
- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Sem nada a apagar (os números mudam com o que há em disco):

```omhl-example fixture=checks
$ omhl byproduct prune
✔ byproduct prune · 0 removed · 0 kept live · 0 bytes freed
```

A mesma poda para máquina:

```omhl-example fixture=checks
$ omhl byproduct prune --format json
{"freed_bytes":0,"kept_live":0,"removed":0}
```

`omhl byproduct` sem argumentos continua sendo erro de uso (saída 2), e `omhl byproduct <check> <work-item>` continua lendo a saída ([byproduct.md](byproduct.md)).
