# JSON Schema Test Suite, filtered copy

Fixture of the `catalog` unit test `it_should_pass_the_filtered_official_suite_without_skips`.

- Repository: `json-schema-org/JSON-Schema-Test-Suite`
- Commit: `245a45abc90190c0bff04d97e6d2b541b4bb9f12` (2026-11-03)
- Source path: `tests/draft2020-12/*.json` (`optional/` and `proposals/` are out of scope)
- Copied on: 2026-11-07
- License: MIT, Copyright (c) 2012 Julian Berman (see `LICENSE`)

Regenerate, from a checkout of the repository at the commit above:

```
python3 -I crates/hexlog/tests/fixtures/json-schema-test-suite/filter.py <checkout> crates/hexlog/tests/fixtures/json-schema-test-suite
```

The filter keeps a group when its schemas use only the keywords of the supported subset, with internal
resolvable `$ref`, `$schema` only at the root with the 2020-12 URI, and only safe-integer numbers
(at most 2^53 - 1 in absolute value, no fraction or exponent). It keeps a test when its `data` has no
number outside that guard. Files are compact JSON with the original names.

Totals: 21 files, 124 groups, 392 tests. The unit test asserts these counts before running anything.

| File | Groups | Tests |
|---|---|---|
| `additionalProperties.json` | 5 | 8 |
| `allOf.json` | 11 | 22 |
| `anyOf.json` | 8 | 16 |
| `boolean_schema.json` | 2 | 18 |
| `const.json` | 15 | 37 |
| `default.json` | 3 | 7 |
| `enum.json` | 16 | 45 |
| `infinite-loop-detection.json` | 1 | 2 |
| `items.json` | 5 | 12 |
| `maxItems.json` | 1 | 4 |
| `maxLength.json` | 1 | 5 |
| `maximum.json` | 1 | 1 |
| `minItems.json` | 1 | 4 |
| `minLength.json` | 1 | 5 |
| `minimum.json` | 1 | 5 |
| `not.json` | 8 | 38 |
| `oneOf.json` | 11 | 25 |
| `properties.json` | 5 | 20 |
| `ref.json` | 12 | 30 |
| `required.json` | 5 | 18 |
| `type.json` | 11 | 70 |
