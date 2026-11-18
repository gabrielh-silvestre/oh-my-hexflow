---
status: accepted
---

# `executor` e `loader` como fronteira da guarda

## Contexto

O `omhl` executa o que está no banco, não o que está no TOML: o arquivo é proposta e o `omhl load` aprova. Sem uma restrição de autoria, qualquer cliente (a CLI) poderia gravar à mão um check com `run`, ou a attestation de um check com `run` com os campos de conteúdo forjados. Só o freio de conteúdo (a attestation exige `byproduct`, `command` e `exit_code`) deixaria qualquer cliente forjar esses campos. O dono decidiu em 2026-11-10 aceitar a restrição por autor. Alternativa mais forte que esse freio, considerada ou descartada: não encontrada no registro.

## Decisão

A guarda (`crates/omhl/src/guard/append.rs`) separa três autores:

- `loader` (`Caller::loader()`, usado por `omhl load` e `omhl check retire`) é o único que grava, revisa ou retira um check com `run`. A revisão de um check cuja versão vigente tem `run` também exige `loader`, mesmo quando a nova versão tira o `run`.
- `executor` (`Caller::executor()`, usado só por `omhl check run`) é o único que grava a attestation de um check com `run`. O `omhl attest` pela CLI é recusado para esses checks (`check-run-author`). A attestation precisa ainda de `byproduct`, `command`, `spec_digest` e, para `passed` e `passed_with_warning`, de `exit_code` (`attestation-incomplete`).
- `guideline-retire` (`Caller::guideline_retirer`, usado só por `omhl guideline retire`) é o único `client` que retira uma diretiva; a retirada por outro `client` é recusada (`guideline-retire-by-command`). Ao contrário dos outros dois, que têm `agent` sintético, este mantém o `agent` real (`user` ou `claude`) e põe no `client` o caminho de escrita. Fora da retirada de diretiva, ele segue as regras de qualquer cliente.

O autor da attestation do `check run` é sempre `executor`, também quando a ação chega pela CLI ou pelo hook.

## Consequências

- Exigir `loader` na revisão e na retirada fecha dois desvios: sem isso, `checks.lint` viraria texto e seria atestado à mão, ou o gate deixaria de cobrá-lo.
- É um freio, não uma parede. O agente que chama a CLI pelo Bash continua indistinguível do usuário, e o agente ainda pode rodar `omhl check run`. Não foi verificado se ele consegue gravar direto no arquivo do banco. Atualizado em parte pelo [ADR 0008](0008-papel-do-agente-imposto-por-hook-e-matriz.md): para o agente portado, o record sai com `agent: "omhl:<papel>"` e a guarda aplica a matriz do papel; contra a má-fé a frase continua valendo.
- O `spec_digest` de cada attestation prende o veredito à definição vigente do check, e só o `executor` pode gravá-lo. Quem edita o TOML, roda `omhl load`, roda o `check run` e restaura o arquivo obtém uma attestation válida para a definição que ficou no banco: o `spec_digest` fecha o caso sem má-fé, não o bypass deliberado.
- Toda fixture de teste que grava um check com `run` usa o autor `loader`, e toda attestation desse check usa `executor`.
- Toda fixture de teste que retira uma diretiva usa o autor `guideline-retire`.
