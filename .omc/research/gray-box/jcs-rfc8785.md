# JCS (RFC 8785) próprio no hexlog: o que a norma manda e o que foi medido

## Tabela de Conteúdos

- [Método de verificação](#método-de-verificação)
- [Ordenação das chaves](#ordenação-das-chaves)
- [Serialização de strings](#serialização-de-strings)
- [Serialização de números](#serialização-de-números)
- [Dígitos mais curtos em Rust](#dígitos-mais-curtos-em-rust)
- [Leitura com serde_json](#leitura-com-serde_json)
- [Inteiros acima de 2^53](#inteiros-acima-de-253)
- [Fontes de vetores](#fontes-de-vetores)
- [Armadilhas conhecidas](#armadilhas-conhecidas)
- [Algoritmo proposto](#algoritmo-proposto)
- [Links Úteis](#links-úteis)

---

## Método de verificação

### O que é?

Consulta de 2026-11-07, feita para o `test-engineer` e o `executor` da T2 implementarem o JCS interno do crate `hexlog` sem refazer a pesquisa. O hash da cadeia, o `def_hash`, a impressão (`fingerprint`) e a âncora são `sha256` de JCS; um byte errado quebra a compatibilidade com o Hexlog TS para sempre. Cada regra abaixo traz a seção da norma e, quando aplicável, uma medição feita aqui.

### O que foi lido e medido?

| item | como |
|---|---|
| RFC 8785, RFC 8259, RFC 7493 | texto integral em `rfc-editor.org` (`.txt`), seções conferidas uma a uma |
| ECMA-262 | rascunho vivo em `tc39.es/ecma262` (título "ECMAScript 2027 Language Specification"), seções lidas do HTML; a numeração do RFC (7.1.12.1, 24.3.2.2) vem de edições antigas, e hoje são 6.1.6.1.20 e 25.5.4.3 |
| Rust | `rustc 1.97.0`, fonte da `core` em `~/.rustup/toolchains/1.97.0-.../library/core/src/num/imp/flt2dec/` e `fmt/float.rs` |
| `serde_json` | 1.0.154, comportamento medido com programas descartáveis |
| Node | v24.18.1 e `canonicalize` 5.1.0 (o do Hexlog TS), como oráculo |
| projeto de testes | `/tmp/claude-1000/-home-gabriel-personal-oh-my-hexflow/c0c83fa7-2673-431f-bc84-73b5401f9c5c/scratchpad/jcs-probe/` (exemplos `jcs.rs`, `es6.rs`, `sj.rs`) |

Resultado de uma frase: o protótipo da seção [Algoritmo proposto](#algoritmo-proposto) passa nos 35 vetores locais (33 reais mais 2 de erro) e em 6.000.000 de linhas do arquivo oficial de números do `cyberphone/json-canonicalization`, e também em 4.499.032 amostras comparadas com o `JSON.stringify` do Node (0 diferenças). O que não passa é o uso direto de `format!("{:e}")` (ver [Dígitos mais curtos em Rust](#dígitos-mais-curtos-em-rust)).

### Como aplicar?

Trate esta nota como especificação de teste: cada regra tem um vetor ou uma medição que o `test-engineer` pode transformar em teste. Itens marcados "não verificado" ou "decisão do dono" não têm resposta fechada aqui.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Ordenação das chaves](https://www.rfc-editor.org/rfc/rfc8785#section-3.2.3)

### O que é?

RFC 8785 §3.2.3: as propriedades de todo objeto, recursivamente, saem ordenadas pelos **code units UTF-16** das chaves "cruas" (já sem escape), comparados como inteiros sem sinal, sem locale. Arrays não são reordenados, mas os objetos dentro deles são. A chave que é prefixo de outra vem antes (`""`, `"a"`, `"aa"`, `"ab"`).

### O que faz?

Em Rust a ordem de `str` (`Ord`) é por bytes UTF-8, igual à ordem por code point, e **não** é a ordem UTF-16. As duas divergem quando se compara um caractere suplementar (U+10000 em diante, que em UTF-16 vira o par D800..DFFF) com um caractere BMP de U+E000 a U+FFFF. O RFC avisa (§3.2.3, "Note") que a saída por UTF-8 seria incompatível nesse caso.

Medido com `sort()` contra `sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()))`:

| ordem | resultado |
|---|---|
| `str::cmp` (UTF-8/code point) | `z`, U+E000, U+FB33, U+FFFF, U+10000, U+1F600 |
| UTF-16 (a que o RFC exige) | `z`, U+10000, U+1F600, U+E000, U+FB33, U+FFFF |

Vetor do próprio RFC (§3.2.3), em que o emoji U+1F600 vem **antes** de U+FB33: chaves `"€"`, `"\r"`, `"דּ"`, `"1"`, `"😀"`, `"\u0080"`, `"ö"` saem na ordem `\r`, `1`, `\u0080`, `ö`, `€`, emoji, `דּ`. Esse vetor está no arquivo local como `rfc8785-3.2.3-utf16-key-order`.

### Como aplicar?

```rust
let mut keys: Vec<&String> = map.keys().collect();
keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
```

`serde_json::Map` sem `preserve_order` é um `BTreeMap<String, _>` ordenado por bytes UTF-8, então a ordem de iteração **não serve**; ordene sempre explicitamente. Chaves duplicadas não existem depois de parseadas num `Map` (ver [Leitura com serde_json](#leitura-com-serde_json)). Otimização opcional, não medida: a ordem por bytes coincide com a UTF-16 quando nenhuma chave contém caractere a partir de U+E000.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Serialização de strings](https://www.rfc-editor.org/rfc/rfc8785#section-3.2.2.2)

### O que é?

RFC 8785 §3.2.2.2, que remete a `QuoteJSONString` do ECMA-262 (hoje §25.5.4.3, com a tabela de escapes "JSON Single Character Escape Sequences" e a operação `UnicodeEscape` §25.5.4.4). Vale para valores string **e** para nomes de propriedade.

### O que faz?

| code point | saída | fonte |
|---|---|---|
| U+0008, U+0009, U+000A, U+000C, U+000D | `\b`, `\t`, `\n`, `\f`, `\r` | RFC §3.2.2.2; ECMA tabela de escapes |
| demais U+0000 a U+001F | `\u00xx` com **hexadecimal minúsculo** (`\u000b`, `\u001f`, `\u0000`) | RFC §3.2.2.2; ECMA `UnicodeEscape` ("lowercase hexadecimal") |
| U+0022 `"` | `\"` | RFC §3.2.2.2 |
| U+005C `\` | `\\` | RFC §3.2.2.2 |
| tudo o mais | **literal**, em UTF-8 | RFC §3.2.2.2 e §3.2.4 |

Nunca se escapa: `/` (o vetor `\u000b ... \"\\/` do arquivo local mantém `/` cru), U+007F, U+2028, U+2029, nenhum caractere não ASCII (a RFC 8259 §7 só obriga a escapar `"`, `\` e U+0000 a U+001F), e caractere suplementar sai como o próprio caractere, nunca como par `😀`. Nenhuma normalização Unicode é feita (RFC 8785 §3.1: preservar "as is"; vetor `cyberphone-unicode` com "Å" não normalizado).

**Surrogate solto.** RFC 8785 §3.2.2.2 ("Note"): lone surrogate "MUST cause a compliant JCS implementation to terminate with an appropriate error". Um `String`/`&str` do Rust não pode conter surrogate (é UTF-8 válido), então a serialização em si nunca vê um. A barreira é a **leitura**: medido com `serde_json` 1.0.154 sobre `Value`:

| entrada | resultado |
|---|---|
| `"\ud800"` | erro `unexpected end of hex escape` |
| `"\udc00"` | erro `lone leading surrogate in hex escape` |
| `"\ud800A"` | erro `unexpected end of hex escape` |
| `"\ude00\ud83d"` (ordem invertida) | erro `lone leading surrogate in hex escape` |
| par válido `"😀"` ou 😀 direto | `String` com U+1F600 |
| bytes `ED A0 80` (surrogate em UTF-8) via `from_slice` | erro `invalid unicode code point` |
| byte `FF` via `from_slice` | erro `invalid unicode code point` |

**Noncharacters** (U+FFFE, U+FFFF, U+FDD0..U+FDEF etc.). RFC 7493 §2.1 diz que I-JSON "MUST NOT" incluí-los, mas RFC 8785 só manda errar em surrogate solto. Os implementadores de referência não os rejeitam. Decisão do dono: aceitar e documentar (o que o RFC 8785 literalmente pede) ou recusar na validação de `data`.

### Como aplicar?

```rust
fn quote(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{0c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => write!(out, "\\u{:04x}", c as u32).unwrap(), // {:04x} = minúsculo
            c => out.push(c),
        }
    }
    out.push('"');
}
```

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Serialização de números](https://www.rfc-editor.org/rfc/rfc8785#section-3.2.2.3)

### O que é?

RFC 8785 §3.2.2.3: todo número é serializado como `Number::toString` do ECMA-262 (RFC cita §7.1.12.1; hoje é §6.1.6.1.20), **incluindo o "Note 2"**. O valor tem de ser um double IEEE 754 (RFC §3.1). NaN e Infinity "MUST cause ... an error". O RFC não reproduz o algoritmo; aponta a implementação do V8 e o Ryu como referências.

### O que faz?

Passo a passo do `Number::toString(x, 10)` do rascunho atual do ECMA-262:

1. `x` é NaN: "NaN" (no JCS, erro).
2. `x` é +0 ou -0: devolve `"0"`. Isso vale também para `-0.0`.
3. `x < 0`: `"-"` seguido de `toString(-x)`.
4. `x` é +∞: "Infinity" (no JCS, erro).
5. Sejam `n`, `k`, `s` inteiros com `k ≥ 1`, `10^(k-1) ≤ s < 10^k`, `s × 10^(n-k)` igual a `x` como double, e `k` o menor possível. `k` é o número de dígitos de `s`; `s` não é múltiplo de 10; o último dígito de `s` não é determinado de forma única só por esses critérios.
6. Se `n` está em [-5, 21] (equivale ao "-6 < n ≤ 21" do ES2015):
   - se `n ≥ k`: os `k` dígitos de `s` seguidos de `n - k` zeros (sem ponto);
   - senão se `0 < n`: os primeiros `n` dígitos, ponto, os `k - n` restantes;
   - senão (`n ≤ 0`): `"0."`, `-n` zeros, os `k` dígitos.
7. Caso contrário (notação científica): sinal do expoente `+` se `n ≥ 1`, `-` se `n < 1`; se `k = 1`: dígito, `e`, sinal, `abs(n - 1)`; se `k > 1`: primeiro dígito, ponto, `k - 1` dígitos restantes, `e`, sinal, `abs(n - 1)`. O expoente nunca tem zeros à esquerda e o sinal `+` é obrigatório.

**Note 2 (obrigatório no JCS):** se houver mais de um `s` possível no passo 5, escolha o `s` cujo `s × 10^(n-k)` fica **mais próximo** de `x`; se dois `s` empatam, escolha o **par** (último dígito par). O ECMA-262 chama isso de recomendação; o RFC 8785 o torna obrigatório ("including the Note 2 enhancement").

Fronteiras (confirmadas nos vetores do Apêndice B): `1e21` vira `"1e+21"` e `999999999999999900000` fica por extenso; `1e-6` vira `"0.000001"` e `9.999999999999997e-7` fica em notação científica; `1e-7` vira `"1e-7"`; `1e23` vira `"1e+23"`; menor positivo `"5e-324"`; maior `"1.7976931348623157e+308"`.

### Como aplicar?

A tradução dos passos 5 a 7 sobre os dígitos de `{:e}` (n = expoente + 1, k = quantidade de dígitos) está na seção [Algoritmo proposto](#algoritmo-proposto). O que decide os dígitos corretos é o assunto da próxima seção.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Dígitos mais curtos em Rust](https://doc.rust-lang.org/std/fmt/trait.LowerExp.html)

### O que é?

A pergunta era se `format!("{:e}", x)` já entrega o `s` do passo 5. A documentação pública de `LowerExp` não promete "mais curto"; a promessa está no comentário de módulo de `core::num::imp::flt2dec` (Rust 1.97; em versões antigas o caminho era `core/src/num/flt2dec`): no modo shortest, o resultado é a representação decimal que (a) arredonda de volta ao mesmo double, (b) é a **mais curta** possível e (c) é a **mais próxima** do valor original, com "some tie-breaking mechanism" nos empates. `fmt/float.rs` chama `to_shortest_exp_str` com `grisu::format_shortest` (Grisu3 de Loitsch 2010, com Dragon4 de Burger e Dybvig 1996 como alternativa quando Grisu não decide).

### O que faz?

Os critérios (a), (b), (c) casam com o passo 5 mais o início do Note 2, mas **o desempate não casa**. Medido:

| teste | `{:e}` puro | `{:e}` com correção de empate |
|---|---|---|
| 2.499.032 amostras (aleatórias, "humanas" e bordas) contra `JSON.stringify` do Node | 500 diferenças | 0 |
| 4.499.032 amostras (as anteriores mais 2.000.000 propensas a empate) | não medido | 0 |
| primeiras 6.000.000 linhas do `es6testfile100m` (vetor oficial do cyberphone) | **1.486 diferenças** | 0 |

As 500 diferenças da primeira linha foram checadas com `Decimal` exato: todas são empates verdadeiros (o valor exato do double fica no meio entre dois candidatos de `k` dígitos); o Node escolhe o dígito par e o `{:e}` escolhe o de cima (arredondamento "meio para cima"). Exemplos do arquivo oficial (bits em hexa, esperado, `{:e}` puro):

| bits | esperado | `{:e}` puro |
|---|---|---|
| `43143ff3c1cb0959` | `1424953923781206.2` | `1424953923781206.3` |
| `c300ce90f2d10fca` | `-591340196471289.2` | `-591340196471289.3` |
| `c2dbfec8a2081be8` | `-123124406820975.62` | `-123124406820975.63` |
| `427fb72bdcfed480` | `2179473461229.2812` | `2179473461229.2813` |

O primeiro exemplo é o vetor `43143ff3c1cb0959` do **Apêndice B do próprio RFC**: quem usa `{:e}` puro falha no vetor oficial.

Correção que passou nos 6.000.000 de vetores e nas 4,5 milhões de amostras: depois de obter os `k` dígitos do modo shortest, formate de novo no modo exato com `k - 1` casas (`{:.*e}`), que devolve o número de `k` dígitos mais próximo do valor (com empate para o par, comportamento **não documentado** do formatador exato, travado apenas pelos testes); se o resultado difere e ainda converte de volta para o mesmo double, use-o.

```rust
fn shortest_even(x: f64) -> String {            // x finito e positivo
    let short = format!("{:e}", x);
    let k = short.split_once('e').unwrap().0.chars().filter(|c| *c != '.').count();
    let exact = format!("{:.*e}", k - 1, x);     // k dígitos, mais próximo, empate ao par
    if exact != short && exact.parse::<f64>().unwrap() == x { exact } else { short }
}
```

Observação derivada por raciocínio, **não medida**: como o `{:e}` puro sempre escolheu o dígito de cima, só pode haver erro quando o último dígito de `short` é ímpar; chamar o modo exato apenas nesse caso cortaria o custo pela metade.

Os quatro algoritmos que a bibliografia nomeia resolvem o mesmo problema com compromissos diferentes: Steele e White 1990 (Dragon4, exato, com inteiros grandes), Burger e Dybvig 1996 (Dragon4 otimizado, usado como alternativa do Rust), Loitsch 2010 (Grisu3: rápido, pode desistir e delegar ao Dragon), Adams 2018 (Ryu: sempre decide; é o que o JCS em Java usa, citado no RFC §3.2.2.3), Giulietti 2020 (Schubfach). Gay 1990 é a referência da Nota 3 do ECMA-262. Nenhum desses é preciso para o hexlog se a correção acima passar nos vetores; eles viram plano B caso uma versão futura do Rust mude o desempate (os vetores de empate avisam).

### Como aplicar?

Use `{:e}` com a correção de `shortest_even` e **trave o comportamento com os vetores de empate** da seção [Fontes de vetores](#fontes-de-vetores). Não escreva Ryu/Schubfach/Grisu próprio, a menos que os vetores falhem numa versão nova do Rust.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Leitura com serde_json](https://docs.rs/serde_json/1.0.154/serde_json/)

### O que é?

O JCS do hexlog recebe `serde_json::Value`. Tudo que o parser faz antes da canonicalização já decide o hash. O plano (linha 191) manda `serde_json` "sem `arbitrary_precision` nem `preserve_order`"; falta a terceira configuração.

### O que faz?

Medido com `serde_json` 1.0.154:

| comportamento | resultado | consequência |
|---|---|---|
| leitura de float sem a feature `float_roundtrip` | **88.269 de 300.000** literais de 17 a 20 dígitos significativos dão double diferente de `str::parse::<f64>` (erro de 1 ulp, ex.: `0.73479537756966477e-11` lê `7.347953775696649e-12`; o correto é `...647e-12`) | o hash do `def_hash` de uma definição com `0.1`, `0.3` etc. pode divergir do Node; ligar `float_roundtrip` zera os erros (0 de 300.000). O Cargo.toml do `serde_json` descreve a feature como "~2x" mais cara no parse de floats e não adiciona crate |
| `-0` (literal inteiro) | vira `f64 -0.0` (`is_f64`), não inteiro 0 | o JCS tem de imprimir `0` (passo 2); a regra do plano (recusar float em `data`) o recusa antes |
| `1e400`, `-1e400` | erro `number out of range` | NaN/Infinity nunca chegam ao canonicalizador pelo `Value` |
| `Number::from_f64(NaN / ±∞)` | `None` | idem |
| `100000000000000000000` (1e20) | vira `f64` 1e20 | o JCS imprime `100000000000000000000` (n = 21, dentro do limite) |
| `9007199254740993` | `u64` exato; `as_f64()` devolve 9007199254740992 | ver [Inteiros acima de 2^53](#inteiros-acima-de-253) |
| chaves duplicadas `{"a":1,"a":2}` | **o último vence, sem erro** | RFC 8785 §3.1 / RFC 7493 §2.3 proíbem duplicadas; o Hexlog TS (`JSON.parse`) também deixa o último vencer. Decisão do dono: rejeitar (exige `Deserialize` próprio) ou aceitar e documentar |
| `Display` de `Number` (e `to_string()` de `Value`) | `1e20` sai `1e+20`; `1.0` sai `1.0`; `-0` sai `-0.0` | **nunca** use a serialização do `serde_json` para o JCS; só `es_number` |
| `serde_json::Map` | ordenado por bytes UTF-8 (`BTreeMap`), ou por inserção com `preserve_order` | não serve como ordem JCS; ordene à parte |

Aviso de feature unification: o Cargo une as features de `serde_json` entre todos os dependentes do workspace. Um crate de terceiros que ligue `preserve_order` ou `arbitrary_precision` muda o `Value` para todos. Confira com `cargo tree -e features -i serde_json` (a T1 já faz o `cargo tree`; o `architecture.rs` pode exigir `float_roundtrip` e proibir as outras duas). `arbitrary_precision` muda o tipo interno de `Number` e quebra `as_u64`/`as_f64` como usados acima.

### Como aplicar?

```toml
serde_json = { version = "1", features = ["float_roundtrip"] }   # sem arbitrary_precision, sem preserve_order
```

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Inteiros acima de 2^53](https://www.rfc-editor.org/rfc/rfc7493#section-2.2)

### O que é?

RFC 8785 §3.1: número "MUST be expressible as IEEE 754 double-precision"; para precisão ou inteiros maiores, "RECOMMENDED" usar string (Apêndice D: números sem lugar natural em JSON "MUST be wrapped using the JSON string type"). RFC 7493 §2.2: inteiro com valor absoluto maior que 9007199254740991 (2^53 - 1) não pode ser tratado pelo receptor como exato. Todo inteiro com módulo até 2^53 inclusive é representável exatamente num double.

### O que faz?

O plano (linha 535) recusa em `data` qualquer número que não seja inteiro seguro (`i64`/`u64` dentro de ±(2^53 - 1)), então `data` nunca chega ao canonicalizador com float ou inteiro grande. Mas o `def_hash` é o JCS de uma definição de catálogo, que pode conter número fracionário (por exemplo `"minimum": 0.5`), e a impressão cobre os itens "como chegaram". Então o canonicalizador precisa de regra própria para os dois casos:

| entrada | regra proposta | por quê |
|---|---|---|
| `u64`/`i64` com módulo ≤ 2^53 | imprimir os dígitos | idêntico ao `Number::toString` até 1e21 |
| `u64`/`i64` com módulo > 2^53 | **erro** (`IntegerOutOfRange`) | converter para `f64` arredondaria em silêncio; `JSON.parse` do Node faz exatamente isso (9007199254740993 vira ...992), então a cadeia TS e a Rust divergiriam sem aviso |
| float finito | `es_number` | seção [Dígitos mais curtos em Rust](#dígitos-mais-curtos-em-rust) |
| float não finito | erro | RFC §3.2.2.3 |

O limite do canonicalizador é 2^53 e não 2^53 - 1 porque o vetor oficial `4340000000000000` (9007199254740992, "Max pos int" do Apêndice B) tem de passar; a regra mais estrita (±(2^53 - 1)) fica na validação de `data`, como o plano já faz. O protótipo rejeitou 2^53 numa primeira versão e falhou exatamente nesse vetor.

### Como aplicar?

Faça o canonicalizador RFC-fiel (aceita |n| ≤ 2^53, recusa o resto) e deixe a política de "só inteiro seguro" na validação de `data`. Um teste por linha da tabela acima.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Fontes de vetores](https://github.com/cyberphone/json-canonicalization)

### O que é?

Três fontes, mais uma conferência cruzada.

| fonte | o que tem | licença e versão |
|---|---|---|
| RFC 8785 | Apêndice B (tabela de 26 números, incluindo as bordas e 2 casos de erro NaN/Infinity), §3.2.2 (exemplo `numbers`/`string`/`literals`), §3.2.3 (ordem UTF-16) | RFC |
| `cyberphone/json-canonicalization`, `testdata/` | `input/` e `output/` (arrays, french, structures, unicode, values, weird) e `outhex/`; `numgen.js`/`numgen.go`; README com o arquivo de números | Apache-2.0; commit consultado `32a48195b9b20fe67b1795e39e4509f14d860b45` (2026-10-11, merge do PR 38 "Reject a surrogate pair that is not a pair, and invalid UTF-8"); 155 estrelas |
| `es6testfile100m.txt.gz` (release `es6testfile` do mesmo repo) | 100 milhões de linhas `hex-ieee,esperado` (valores aleatórios e de borda, gerados de forma determinística) | 2.081.240.993 bytes, última modificação 2021-12-06; linhas de exemplo no README: `4340000000000001,9007199254740994`, `444b1ae4d6e2ef50,1e+21`, `3eb0c6f7a0b5ed8d,0.000001`, `3eb0c6f7a0b5ed8c,9.999999999999997e-7` |
| `canonicalize` npm 5.1.0 em `/home/gabriel/personal/hexlog/node_modules/canonicalize` (repo `erdtman/canonicalize`) | oráculo do Hexlog TS; usa `JSON.stringify` para strings e números, com `isWellFormed()` que lança em surrogate solto, e lança em NaN/Infinity | Apache-2.0 |

O arquivo local `/tmp/claude-1000/-home-gabriel-personal-oh-my-hexflow/c0c83fa7-2673-431f-bc84-73b5401f9c5c/scratchpad/t1-spikes/rfc8785_vectors.json` tem 35 vetores com campos `name`, `ieee754`, `input`, `expected`: 26 do Apêndice B (2 deles com `input: null` e `expected: null`, os casos NaN/Infinity, que só podem ser testados chamando o serializador de número com `f64::NAN`/`f64::INFINITY`), 6 do cyberphone (arrays, french, structures, unicode, values, weird), 1 de §3.2.4, 1 de §3.2.3 e 1 de §3.2.2.2 (escapes de controle).

### O que faz?

Conferências feitas:

- O protótipo de [Algoritmo proposto](#algoritmo-proposto) passa os 35 vetores locais (33 reais + 2 que só testam erro).
- O `canonicalize` 5.1.0 do Hexlog TS concorda com 33 dos 35 vetores; os 2 que a harness acusou são os de `input: null` (NaN/Infinity), que não são representáveis como texto JSON. Ele é um oráculo válido para a conferência cruzada.
- 6.000.000 das 100 milhões de linhas do arquivo oficial passam com 0 diferenças usando a correção de empate; `{:e}` puro falha em 1.486 delas.

Dica de teste: o arquivo completo tem 2 GB compactado; para a suíte do repositório, não o versione. Extraia as linhas que o `{:e}` puro erra (1.486 nas primeiras 6 milhões, a maioria empates) e as bordas do Apêndice B para um arquivo pequeno com cabeçalho de atribuição Apache-2.0, ou gere um conjunto com o `numgen.js` (algoritmo no README do cyberphone). Um teste `#[ignore]` que lê o arquivo completo por caminho em variável de ambiente cobre o resto.

### Como aplicar?

`jcs_vectors` da T2 (plano, linhas 749-753) usa o arquivo local de 35 vetores; acrescente (1) os vetores de empate da seção anterior e (2) um teste que compara, para N doubles aleatórios, a saída do serializador de número com `node -e 'console.log(JSON.stringify(x))'` (Node 24 disponível; foi o método da validação desta pesquisa). Para strings e chaves, os 3 vetores do RFC (§3.2.2 exemplo, §3.2.3 ordem, §3.2.2.2 controles) mais os 6 do cyberphone.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Armadilhas conhecidas

### O que é?

Lista de erros que implementações reais cometem, cada um com a evidência de onde veio.

### O que faz?

| # | armadilha | evidência |
|---|---|---|
| 1 | `format!("{:e}")` puro erra os empates (arredonda para cima, o JCS exige o par) | 1.486 diferenças em 6.000.000 de vetores oficiais; falha no vetor `43143ff3c1cb0959` do Apêndice B |
| 2 | `serde_json` sem `float_roundtrip` lê floats de 17+ dígitos com erro de 1 ulp | 88.269 de 300.000 casos diferentes de `str::parse::<f64>` |
| 3 | Usar `Value::to_string()`/`Display` do `serde_json` para números (`1e+20`, `1.0`, `-0.0` saem diferentes do ES) | medido na seção de leitura |
| 4 | Ordenar chaves pela ordem do `Map` ou por `str::cmp` em vez de UTF-16 | tabela de ordenação; vetor §3.2.3 do RFC |
| 5 | `-0` e `-0.0` imprimirem `-0` em vez de `0` | passo 2 do `Number::toString`; vetor `8000000000000000` do Apêndice B |
| 6 | Inteiro acima de 2^53 convertido para double sem aviso; `JSON.parse` faz isso e o `u64` do Rust não | `9007199254740993` lido como `u64` exato |
| 7 | Limite de 2^53 aplicado antes do vetor `4340000000000000` (9007199254740992) | o protótipo falhou aí na primeira versão |
| 8 | `JSON.stringify` puro **não** é conforme para surrogate solto: desde o ES2019 ele escapa o surrogate (`\udead`, minúsculo) em vez de dar erro (ECMA §25.5.4.3, ramo "leading surrogate or trailing surrogate"). O `canonicalize` 5.1.0 evita isso com `isWellFormed()` | texto do rascunho do ECMA-262; fonte do `canonicalize.js` |
| 9 | Até o implementador de referência erra Unicode: o commit de 2026-10-11 do cyberphone corrigiu um par de surrogates invertido emitido como U+FFFD e um byte `0xFF` copiado para a saída | commit `0c88af0958796de49dea4594a237fdee7c9014a3` |
| 10 | Escapar o que não se escapa (`/`, U+007F, U+2028/2029, caracteres não ASCII) ou emitir `\u00XX` em maiúsculas | RFC 8785 §3.2.2.2; vetor de controles |
| 11 | Aceitar chave duplicada em silêncio (último vence) | `serde_json` e `JSON.parse` fazem isso; RFC 8785 §3.1 proíbe |
| 12 | Normalizar Unicode (NFC) ou trocar a string | RFC 8785 §3.1 ("MUST preserve ... as is") |
| 13 | Recursão ilimitada em JSON aninhado | o `serde_json` limita a 128 níveis por padrão na leitura (conhecimento da biblioteca, não medido aqui); o canonicalizador deve ter profundidade limitada ou ser iterativo. Não verificado se o plano define esse limite |
| 14 | Feature unification ligando `preserve_order` ou `arbitrary_precision` por dependência de terceiros | ver [Leitura com serde_json](#leitura-com-serde_json) |

### Como aplicar?

Cada linha vira pelo menos um teste nomeado no estilo `it_should_...`; as linhas 1, 2 e 7 já têm vetor pronto.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Algoritmo proposto

### O que é?

Função `canonical(&Value) -> Result<String, CanonicalError>` em `chain.rs`, cuja saída em UTF-8 (`String::into_bytes`) é o único insumo do `sha256`. Validado como protótipo em `/tmp/claude-1000/-home-gabriel-personal-oh-my-hexflow/c0c83fa7-2673-431f-bc84-73b5401f9c5c/scratchpad/jcs-probe/examples/jcs.rs` (35 vetores) e `examples/es6.rs` (6.000.000 de linhas).

### O que faz?

Passos:

1. **Pré-condição de leitura:** `serde_json` com `float_roundtrip`, sem `arbitrary_precision` nem `preserve_order`. Decidir (dono) se duplicadas são recusadas.
2. **Valores:** `null`, `true`, `false` literais.
3. **Número:** `u64`/`i64` com módulo ≤ 2^53: dígitos decimais; acima: `IntegerOutOfRange`. Float: não finito é erro; zero (inclusive `-0.0`) é `"0"`; senão `es_number`.
4. **`es_number(x)` para `x ≠ 0` finito:** `neg = x < 0`; `e = shortest_even(|x|)` (formato `d.ddde±x`); `digits` = mantissa sem o ponto; `k = len(digits)`; `n = expoente + 1`. Se `k ≤ n ≤ 21`: dígitos mais `n - k` zeros. Senão, se `0 < n ≤ 21`: `digits[..n] + "." + digits[n..]`. Senão, se `-6 < n ≤ 0`: `"0." + zeros(-n) + digits`. Senão: `digits[0]` (+ `"." + digits[1..]` se `k > 1`) + `"e"` + (`"-"` se `n - 1 < 0`, senão `"+"`) + `abs(n - 1)`. Prefixo `"-"` se `neg`.
5. **String e nome de propriedade:** `quote` (seção de strings).
6. **Array:** `[`, elementos canonicalizados em ordem separados por `,`, `]`.
7. **Objeto:** chaves ordenadas por `encode_utf16()`, saída `{` + `quote(chave)` + `:` + valor canonicalizado, separados por `,`, + `}`. Sem espaço em lugar nenhum.
8. **Hash:** `sha256` dos bytes UTF-8 da saída.

Código (trecho que o protótipo validou):

```rust
fn es_number(x: f64) -> String {                       // x finito, != 0
    let neg = x < 0.0;
    let e = shortest_even(x.abs());                    // "d.ddde-x"
    let (mant, exp) = e.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let (k, n) = (digits.len() as i32, exp + 1);
    let mut out = String::new();
    if neg { out.push('-'); }
    if k <= n && n <= 21 {
        out.push_str(&digits); out.extend(std::iter::repeat('0').take((n - k) as usize));
    } else if 0 < n && n <= 21 {
        out.push_str(&digits[..n as usize]); out.push('.'); out.push_str(&digits[n as usize..]);
    } else if -6 < n && n <= 0 {
        out.push_str("0."); out.extend(std::iter::repeat('0').take((-n) as usize)); out.push_str(&digits);
    } else {
        out.push_str(&digits[..1]);
        if k > 1 { out.push('.'); out.push_str(&digits[1..]); }
        write!(out, "e{}{}", if n - 1 < 0 { '-' } else { '+' }, (n - 1).abs()).unwrap();
    }
    out
}
```

`shortest_even` está na seção [Dígitos mais curtos em Rust](#dígitos-mais-curtos-em-rust); `quote`, em [Serialização de strings](#serialização-de-strings).

### Como aplicar?

Ordem sugerida para a T2: (1) `test-engineer` escreve `jcs_vectors` contra os 35 vetores, os de empate e os de erro; (2) `executor` implementa `canonical`; (3) teste diferencial com Node para doubles aleatórios e propensos a empate (inteiros de 2^49 a 2^53 divididos por potências de 2). Pendências para o dono: política de chave duplicada, política de noncharacter, limite de profundidade.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Links Úteis

Todos consultados em 2026-11-07, salvo onde indicado.

- [RFC 8785: JSON Canonicalization Scheme (JCS)](https://www.rfc-editor.org/rfc/rfc8785) (texto em `.txt`; §3.1, §3.2.2.2, §3.2.2.3, §3.2.3, §3.2.4, Apêndices B, D, E, F)
- [RFC 8259: The JSON Data Interchange Format](https://www.rfc-editor.org/rfc/rfc8259) (§4, §6, §7, §8.3)
- [RFC 7493: The I-JSON Message Format](https://www.rfc-editor.org/rfc/rfc7493) (§2.1, §2.2, §2.3)
- [ECMA-262, rascunho vivo (2027)](https://tc39.es/ecma262/) (`Number::toString` §6.1.6.1.20 com Notas 1 a 3; `QuoteJSONString` §25.5.4.3 e `UnicodeEscape` §25.5.4.4)
- [ECMA-262, índice de edições](https://262.ecma-international.org/) (o RFC 8785 cita a 10ª edição, 2019)
- [Rust: trait `LowerExp`](https://doc.rust-lang.org/std/fmt/trait.LowerExp.html)
- Fonte do Rust 1.97 (local): `library/core/src/fmt/float.rs` e `library/core/src/num/imp/flt2dec/{mod.rs,strategy/grisu.rs,strategy/dragon.rs}`
- [serde_json 1.0.154](https://docs.rs/serde_json/1.0.154/serde_json/) e [Cargo.toml com a descrição de `float_roundtrip`, `preserve_order`, `arbitrary_precision`](https://github.com/serde-rs/json/blob/master/Cargo.toml)
- [cyberphone/json-canonicalization](https://github.com/cyberphone/json-canonicalization) (Apache-2.0; commit `32a48195b9b20fe67b1795e39e4509f14d860b45`), [testdata](https://github.com/cyberphone/json-canonicalization/tree/32a48195b9b20fe67b1795e39e4509f14d860b45/testdata) e [arquivo de 100 milhões de números](https://github.com/cyberphone/json-canonicalization/releases/download/es6testfile/es6testfile100m.txt.gz)
- [erdtman/canonicalize](https://github.com/erdtman/canonicalize) (npm `canonicalize` 5.1.0, Apache-2.0)
- [Unicode Standard, capítulo 3 (conformidade, surrogates)](https://www.unicode.org/versions/Unicode16.0.0/core-spec/chapter-3/)
- Steele e White, "How to print floating-point numbers accurately", PLDI 1990 (https://dl.acm.org/doi/10.1145/93542.93559; o site respondeu 403 ao acesso automatizado, bibliografia não conferida na fonte)
- Burger e Dybvig, "Printing floating-point numbers quickly and accurately", PLDI 1996 (citado no cabeçalho de `dragon.rs` do Rust 1.97; https://dl.acm.org/doi/10.1145/249069.231397, 403 ao acesso automatizado)
- Loitsch, "Printing floating-point numbers quickly and accurately with integers", PLDI 2010 (citado no cabeçalho de `grisu.rs`; https://dl.acm.org/doi/10.1145/1809028.1806623, 403 ao acesso automatizado)
- Adams, "Ryu: fast float-to-string conversion", PLDI 2018 (https://dl.acm.org/doi/10.1145/3192366.3192369, 403 ao acesso automatizado; o RFC 8785 §3.2.2.3 cita o Ryu como referência compatível)
- Giulietti, "The Schubfach way to render doubles", 2020 (título e autoria não conferidos na fonte; implementação de referência em C++: https://github.com/vitaut/schubfach)
- Gay, "Correctly Rounded Binary-Decimal and Decimal-Binary Conversions", AT&T Bell Labs, 1990 (citado na Nota 3 do ECMA-262; https://ampl.com/_archive/first-website/REFS/rounding.pdf respondeu 403)
