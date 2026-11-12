---
status: accepted
---

# Saída dos checks em blobs fora do banco

## Contexto

O `check run` captura o `stdout` e o `stderr` do comando, até 4 MiB por fluxo, e o agente precisa ler essa saída para corrigir um check que falhou. O `hexlog` guarda a chain num SQLite append-only e valida cada record contra o catalog; ele não tem blob store, e um campo `format: attachment` só valida um hash de 64 caracteres hexadecimais. O limite padrão de `data` por record é de 16.000 caracteres (`Limits::data_max_chars`), muito abaixo do teto de saída do runner. O dono aceitou em 2026-11-10 o campo `byproduct` da attestation (vocabulário do SLSA Provenance) como o hash desse arquivo.

## Decisão

A saída completa vai para um arquivo em `<pasta do banco>/blobs/<2 primeiros hex>/<sha256>` (`ports/blob`, porta `ByproductStore`), e a attestation grava só o hash no campo `byproduct`. O `check run` grava o arquivo antes do `append` da attestation. O gate depende só do `outcome`, então a attestation segue válida se o arquivo sumir.

## Consequências

- O banco guarda só o hash, e a saída não passa a fazer parte da chain.
- Se o `omhl` não consegue gravar o arquivo, o `check run` para tudo (`blob-failed`): sem a saída o veredito não seria recuperável pelo agente.
- A leitura confere o SHA-256 do conteúdo; um arquivo adulterado ou truncado vira `byproduct-corrupt`, e um arquivo ausente vira `byproduct-missing`.
- A gravação é atômica (nome temporário e `rename`), com pasta `0700` e arquivo `0600`. Uma interrupção entre a gravação do arquivo e a da attestation só pode deixar um arquivo sem uso.
- Não há coleta de lixo: nenhum comando apaga os arquivos antigos, e a saída não passa por redação de segredos. O dono apaga `blobs/` à mão.
