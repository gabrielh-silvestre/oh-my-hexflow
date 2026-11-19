/// Onde uma linha do texto está em relação às cercas de código.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FenceLine {
    Outside,
    /// A linha que abre a cerca (começa com três crases, depois de espaços).
    Open,
    Inside,
    /// A linha que fecha a cerca.
    Close,
}

/// Cada linha do texto com seu lugar em relação às cercas: a cerca abre e fecha em toda linha que começa com três crases.
pub fn lines_by_fence(text: &str) -> impl Iterator<Item = (FenceLine, &str)> {
    let mut inside = false;

    text.lines().map(move |line| {
        let is_fence = line.trim_start().starts_with("```");
        let place = match (is_fence, inside) {
            (true, false) => FenceLine::Open,
            (true, true) => FenceLine::Close,
            (false, true) => FenceLine::Inside,
            (false, false) => FenceLine::Outside,
        };
        inside ^= is_fence;

        (place, line)
    })
}
