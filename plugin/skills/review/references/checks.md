# Checks mecânicos

Um check com o campo `run` é mecânico: o `omhl check run` executa o comando, compara o resultado com o `expect`, grava uma attestation (`passed`, `passed_with_warning` ou `failed`) e guarda a saída do comando. O gate de pull request lê só o desfecho. O que fazer quando o check ou o hook recusa está em [gates-operation.md](gates-operation.md). As regras que valem o tempo todo estão no `SKILL.md` da `review`.

## Regras do check

- **Um comando por check.** Não junte comandos num `run` (`sh -c "a && b"`, `;`, `2>&1`). Ordene vários checks com o campo `after`.
- **O desfecho é imutável.** O mesmo snapshot, check e work item não troca de desfecho. Só uma árvore nova (um commit novo) libera um veredito novo.
- **Check somente leitura.** Se o comando altera a árvore, o `check run` para com `check-not-run`. Ponha o que ele gera no `.gitignore`.
- **A `main` não recebe commit nem push direto.** O branch de trabalho segue `tipo/wi-<id>__<slug>`.

## O ciclo

1. `omhl hooks install`, uma vez por clone: liga o `pre-commit` (evento `before-commit`) e o `pre-push` (evento `before-pr`).
2. `omhl load` a cada arquivo criado, editado ou apagado em `.omhl/`.
3. `git commit` e `git push` rodam o `omhl check run` sozinhos e barram em falha. O `before-merge` não tem hook: rode `omhl check run --event before-merge` à mão. Check manual (sem `run`) é do `omhl attest`, depois de o humano revisar.
4. Check reprovado: leia `omhl byproduct <check> <work-item> --tail 40`, corrija e faça um commit novo (o desfecho do mesmo snapshot não muda).
5. `omhl check retire` só quando a remoção do arquivo do check já foi mergeada; a varredura global (`check sweep`) só depois de um `omhl` interrompido, com a aprovação do humano.

## Páginas

| Comando | Página |
|---|---|
| Carregar os checks no banco | [load.md](load.md) |
| Rodar os checks | [check-run.md](check-run.md) |
| Varrer processos fugidos | [check-sweep.md](check-sweep.md) |
| Aposentar um check | [check-retire.md](check-retire.md) |
| Ler a saída de um check | [byproduct.md](byproduct.md) |
| Instalar os hooks de git | [hooks-install.md](hooks-install.md) |
| Hook `post-commit` | [hooks-post-commit.md](hooks-post-commit.md) |
| Atestar um check manual | [attest.md](attest.md) |
| Quando o check ou o hook recusa | [gates-operation.md](gates-operation.md) |
| Erros comuns | [errors.md](../../directives-flow/references/errors.md) |
