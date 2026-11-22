---
status: accepted
---

# Saída dos checks em blobs fora do banco

## Contexto

O `check run` captura o `stdout` e o `stderr` do comando, até 4 MiB por fluxo, e o agente precisa ler essa saída para corrigir um check que falhou. O `hexlog` guarda a chain num SQLite append-only e valida cada record contra o catalog; ele não tem blob store, e um campo `format: attachment` só valida um hash de 64 caracteres hexadecimais. O limite padrão de `data` por record é de 16.000 caracteres (`Limits::data_max_chars`). O dono aceitou em 2026-11-10 o campo `byproduct` da attestation (vocabulário do SLSA Provenance) como o hash desse arquivo.

Motivo registrado pelo dono para guardar a saída fora do banco: não encontrado. Inferência, não confirmada: o limite de 16.000 caracteres por record é muito menor que o teto de 4 MiB por fluxo do runner, então a saída não caberia no record.

## Decisão

A saída completa vai para um arquivo em `<pasta do banco>/blobs/by-db/<arquivo do banco>/<projeto>/<2 primeiros hex>/<sha256>` (`ports/blob`, porta `ByproductStore`), e a attestation grava só o hash no campo `byproduct`. A pasta por banco e projeto mantém a poda de um projeto longe dos arquivos de outro. O `check run` grava o arquivo antes do `append` da attestation. O gate depende só do `outcome`, então a attestation segue válida se o arquivo sumir.

## Consequências

- O banco guarda só o hash, e a saída não passa a fazer parte da chain.
- Se o `omhl` não consegue gravar o arquivo, o `check run` para tudo (`blob-failed`): sem a saída o veredito não seria recuperável pelo agente.
- A leitura confere o SHA-256 do conteúdo; um arquivo adulterado ou truncado vira `byproduct-corrupt`, e um arquivo ausente vira `byproduct-missing`.
- A gravação é atômica (nome temporário e `rename`), com pasta `0700` e arquivo `0600`. Uma interrupção entre a gravação do arquivo e a da attestation só pode deixar um arquivo sem uso.
- **Retenção de 7 dias.** O `check run` apaga, no começo de cada execução, o arquivo de saída com mais de 7 dias de data de modificação, no máximo **uma poda por dia** por (banco, projeto); um carimbo `.pruned-at` na pasta segura as demais. `omhl byproduct prune` faz a mesma poda na hora, ignora o carimbo e devolve `removed`, `kept_live` e `freed_bytes`; é uma folha só da sessão, fora da matriz dos papéis. Uma falha de leitura ou de remoção sai como `byproduct-prune-failed` no comando; no `check run` a poda é silenciosa.
- **O que a poda poupa.** A saída "viva" é a da attestation de maior `seq` de cada par (check, work item): ela não some por idade, então ler a falha vigente não vira `byproduct-missing` por causa da retenção. A de uma árvore anterior do mesmo par some depois de 7 dias.
- **Layout antigo.** Os arquivos de `blobs/<2 primeiros hex>/<sha256>` (antes da pasta por banco e projeto) continuam sendo lidos, mas nunca são podados; a limpeza deles é manual, uma vez. Depois que o `check run` grava o mesmo conteúdo no layout novo, o arquivo pode existir nos dois. Renomear o arquivo do banco muda a pasta: a saída gravada antes do nome novo deixa de ser achada.
- **Redação de segredos.** O `check run` mascara na saída o que casa com a lista de padrões antes de gravar, e a leitura aplica a mesma máscara ao arquivo inteiro antes de cortar as linhas. A lista **nunca cobre todo formato**: um segredo fora dela fica no arquivo, e a saída continua dado não confiável. Os arquivos gravados antes da redação não são reescritos, só lidos já mascarados; por isso o campo `byproduct` da resposta é o hash do arquivo em disco, que pode ser o da versão sem máscara.
