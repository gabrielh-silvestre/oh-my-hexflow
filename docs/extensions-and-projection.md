# Extensions e projeção

Vocabulário em [GLOSSARY.md](../GLOSSARY.md).

## Extensions (arquivo para banco)

Uma extension é escrita à mão e versionada com o repositório.

```bash
omhl load --dry-run
omhl load
```

Retire com `omhl check retire <check-id>` (só para check cujo arquivo sumiu; é definitivo e vale para todos os clones e branches, então só depois de a remoção do arquivo estar mergeada). Trocar de branch não perde nada.

A diretiva órfã sai com `omhl guideline retire <guideline-id>`, com `--by <id-novo>` quando outra a substitui. Vale o mesmo cuidado.

## Projeção (banco para arquivo)

```bash
omhl project
```
