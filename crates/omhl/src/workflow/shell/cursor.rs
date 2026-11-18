/// Posição de leitura sobre o texto do comando, caractere a caractere.
pub(super) struct Cursor {
    chars: Vec<char>,
    at: usize,
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
