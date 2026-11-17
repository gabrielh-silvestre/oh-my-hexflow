// Um binário de teste só: o cargo trata cada `tests/*.rs` como crate; `#[path]` evita `coverage.rs`
// e `examples.rs` virarem dois binários.
mod support;

#[path = "skills/coverage.rs"]
mod coverage;
#[path = "skills/examples.rs"]
mod examples;
