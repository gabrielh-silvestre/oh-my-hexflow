/// Causa e próximo passo de uma recusa; só texto fixo, nunca o valor de entrada.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Guidance {
    pub cause: &'static str,
    pub next_step: &'static str,
}
