---
status: accepted
---

# Minor de uma definition decidida por objetos fechados e regra sintática

O HexCore grava cada definition (tipo de node ou relation type) com versão `major.minor`, e um record validado numa minor tem de continuar válido nas minors seguintes da mesma major. "Só acrescentar sobe a minor" não garante isso quando o objeto do schema é aberto: um record que já trazia `estimativa: "alta"` como chave não declarada fica inválido quando uma minor acrescenta `estimativa` como `integer` opcional. Decidimos que **todo objeto de uma definition é fechado** (`additionalProperties: false`, exigido pelo motor ao definir) e que o motor classifica a mudança por uma **regra sintática** sobre as keywords: propriedade opcional nova, valor novo de enum fora de `not` e de `oneOf`, alargar as pontas de um relation type e mudança só em anotações (`title`, `description`, `$comment`, `default`, `examples`, `deprecated`, `readOnly`, `writeOnly` e o `equivalent` de um tipo) sobem a minor; qualquer outra diferença, inclusive dentro de `oneOf`, `not` ou `$ref`, sobe a major. Os ramos de `allOf`, `anyOf`, `oneOf` e `not` validam o mesmo valor do objeto pai e herdam o fechamento dele: não precisam declarar `additionalProperties: false` quando o pai já é fechado. Anotação é minor porque o validador não a usa para aceitar ou recusar um valor, e o motor não aplica `default` (`crates/hexlog/src/catalog/validate.rs:421`). O motor já tem validador próprio com vocabulário fixo de keywords (`crates/hexlog/src/catalog/validate.rs`), então a regra fica sob controle do próprio motor.

## Considered Options

- **Inclusão provada pelo módulo `canonical` do crate `jsonschema`** (`CanonicalSchema::covers` e `subtract`): o motor provaria que o schema novo aceita tudo que o antigo aceitava, sem travar keywords. Rejeitada por ora porque o módulo é experimental. Situação em 2026-11-10, versão 0.61.1: o código avisa *"Experimental: keyword coverage is incomplete and the API may change in minor releases"*; o módulo surgiu na 0.49.0 (2026-07-25) e `covers` e `subtract` na 0.50.0 (2026-08-20, com `is_subset_of` renomeado para `covers`); houve 34 releases, 3 mudanças que quebraram a API do módulo e 33 correções nele, entre elas `covers` respondendo `Yes` indevidamente (0.50.1). Um `Yes` indevido classificaria uma quebra como minor sem aviso. Também exigiria um segundo validador ao lado do próprio do motor.
- **Validar todos os records existentes do tipo contra o schema novo** ao definir a minor, subindo a major se algum falhar: exata para o histórico real, com custo de ler todos os records do tipo. Não adotada nesta decisão.
- **Manter "só acrescentar = minor" sem fechar objetos**, como o hexlog em TypeScript: rejeitada porque deixa record antigo inválido numa minor.

## Revisitar

Trocar a regra sintática pela inclusão do `canonical` quando o módulo estabilizar. Critérios propostos para considerar estável, conferidos no `CHANGELOG.md` e em `crates/jsonschema/src/canonical/mod.rs` do repositório `Stranger6667/jsonschema`:

1. O aviso "Experimental" sai do módulo `canonical` e da função `canonicalize`.
2. Seis meses sem entrada **BREAKING** que toque `canonical`, `covers` ou `subtract`.
3. Nesse mesmo período, nenhuma correção de `covers` ou `subtract` dando resposta errada.

Ao trocar, os objetos fechados podem deixar de ser obrigatórios, e a mudança da própria regra de classificação é uma decisão nova, registrada em outra ADR que substitui esta.
