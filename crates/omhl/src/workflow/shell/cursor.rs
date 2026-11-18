/// Posição de leitura sobre o texto do comando, caractere a caractere.
pub(super) struct Cursor {
    chars: Vec<char>,
    /// Quantos caracteres já foram consumidos.
    pub(super) at: usize,
}

impl Cursor {
    pub(super) fn new(source: &str) -> Cursor {
        Cursor {
            chars: source.chars().collect(),
            at: 0,
        }
    }

    pub(super) fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    /// O texto cru lido desde o caractere `start`.
    pub(super) fn text_since(&self, start: usize) -> String {
        self.chars[start..self.at].iter().collect()
    }

    /// Logo depois de `$(`: o `(` seguinte abre uma aritmética `$((...))` quando o primeiro `)` de fora
    /// é seguido de outro `)`; senão é `$(` com um subshell, que executa.
    /// Devolve também quantos caracteres a varredura leu, para o leitor limitar o custo total.
    pub(super) fn opens_arithmetic(&self) -> (bool, usize) {
        if self.peek() != Some('(') {
            return (false, 0);
        }
        let rest = &self.chars[self.at + 1..];
        let mut depth = 0;
        for (offset, c) in rest.iter().enumerate() {
            match c {
                '(' => depth += 1,
                ')' if depth > 0 => depth -= 1,
                ')' => return (rest.get(offset + 1) == Some(&')'), offset + 1),
                _ => {}
            }
        }

        (false, rest.len())
    }

    pub(super) fn bump(&mut self) -> Option<char> {
        let next = self.peek();
        self.at += usize::from(next.is_some());

        next
    }

    /// Consome `expected` se for o próximo caractere.
    pub(super) fn eat(&mut self, expected: char) -> bool {
        let found = self.peek() == Some(expected);
        self.at += usize::from(found);

        found
    }

    /// Para o comentário: deixa a quebra de linha para o laço principal.
    pub(super) fn skip_to_newline(&mut self) {
        while self.peek().is_some_and(|c| c != '\n') {
            self.at += 1;
        }
    }

    /// A próxima linha sem a quebra; `None` quando o texto acabou.
    pub(super) fn read_line(&mut self) -> Option<String> {
        self.peek()?;
        let mut line = String::new();
        while let Some(c) = self.bump() {
            if c == '\n' {
                break;
            }
            line.push(c);
        }

        Some(line)
    }
}
