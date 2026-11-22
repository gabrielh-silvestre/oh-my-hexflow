use std::mem;

use super::cursor::Cursor;
use super::heredoc::{self, Delimiter};
use super::redirect::{self, NextWord};
use super::{MAX_COMMAND_BYTES, ReadError};

/// Os comandos simples na ordem de leitura (cada um é a lista das suas palavras, sem aspas) e se algum separador apareceu.
pub(super) struct Lexed {
    pub(super) commands: Vec<Vec<String>>,
    pub(super) separated: bool,
    /// Em que caractere começa a primeira palavra do primeiro comando.
    pub(super) first_start: Option<usize>,
    /// Algum heredoc sem aspas no terminador tem `$(` ou crase no corpo.
    pub(super) executes_heredoc: bool,
}

/// Aninhamentos (`$(`, `(`, crase, `${`, `$((`) abertos ao mesmo tempo; acima disso o leitor desiste.
pub(super) const MAX_DEPTH: usize = 64;

/// Caracteres que as varreduras de `$((` podem ler no total: cada `$((` olha o resto do texto, então sem
/// o limite um comando de `MAX_COMMAND_BYTES` com muitos `$((` custaria tempo quadrático.
pub(super) const MAX_SCAN: usize = 4 * MAX_COMMAND_BYTES;

pub(super) fn lex(source: &str) -> Result<Lexed, ReadError> {
    let mut lexer = Lexer {
        cursor: Cursor::new(source),
        commands: Vec::new(),
        current: Vec::new(),
        word: None,
        word_start: 0,
        depth: 0,
        scanned: 0,
        next_word: NextWord::Argument,
        heredocs: Vec::new(),
        separated: false,
        first_start: None,
        executes_heredoc: false,
    };
    lexer.run(None)?;
    if !lexer.heredocs.is_empty() {
        return Err(ReadError::UnclosedHeredoc);
    }

    Ok(Lexed {
        commands: lexer.commands,
        separated: lexer.separated,
        first_start: lexer.first_start,
        executes_heredoc: lexer.executes_heredoc,
    })
}

#[derive(Default)]
struct WordBuilder {
    text: String,
    quoted: bool,
}

impl WordBuilder {
    /// O número que antecede um redirecionamento (`2>&1`) é o descritor, não um argumento.
    fn is_descriptor(&self) -> bool {
        !self.quoted && !self.text.is_empty() && self.text.chars().all(|c| c.is_ascii_digit())
    }

    /// `{` e `}` soltos, sem aspas, em posição de comando são separadores de grupo.
    fn is_group_brace(&self) -> bool {
        !self.quoted && matches!(self.text.as_str(), "{" | "}")
    }
}

struct Lexer {
    cursor: Cursor,
    commands: Vec<Vec<String>>,
    current: Vec<String>,
    word: Option<WordBuilder>,
    /// Caractere em que a palavra em andamento começou.
    word_start: usize,
    depth: usize,
    /// Caracteres lidos pelas varreduras de `opens_arithmetic` até aqui.
    scanned: usize,
    next_word: NextWord,
    heredocs: Vec<Delimiter>,
    separated: bool,
    first_start: Option<usize>,
    executes_heredoc: bool,
}

impl Lexer {
    /// Lê até o caractere `close` (fora de aspas) ou o fim do texto.
    fn run(&mut self, close: Option<char>) -> Result<(), ReadError> {
        while let Some(c) = self.cursor.bump() {
            if Some(c) == close {
                break;
            }
            self.step(c)?;
        }
        self.end_command();

        Ok(())
    }

    fn step(&mut self, c: char) -> Result<(), ReadError> {
        if self.word.is_none() {
            self.word_start = self.cursor.at - 1;
        }
        match c {
            ' ' | '\t' => self.end_word(),
            '\n' => self.line_break()?,
            '#' if self.word.is_none() => self.cursor.skip_to_newline(),
            '|' | ';' | ')' => self.separator(),
            '&' => self.ampersand()?,
            '(' => {
                self.separator();
                self.nested(')')?;
            }
            '<' | '>' => self.redirect(c)?,
            '\'' => self.single_quoted()?,
            '"' => self.double_quoted()?,
            '\\' => self.escape(),
            _ if self.substitution(c)? => {}
            _ => self.push(c, false),
        }

        Ok(())
    }

    fn push(&mut self, c: char, quoted: bool) {
        let word = self.word.get_or_insert_default();
        word.text.push(c);
        word.quoted |= quoted;
    }

    fn end_word(&mut self) {
        let Some(word) = self.word.take() else {
            return;
        };
        // Palavra sem texto e sem aspas não é argumento: o shell a descarta.
        if word.text.is_empty() && !word.quoted {
            return;
        }
        match mem::take(&mut self.next_word) {
            NextWord::Ignored => {}
            NextWord::Delimiter { strip_tabs } => self.heredocs.push(Delimiter {
                tag: word.text,
                strip_tabs,
                quoted: word.quoted,
            }),
            NextWord::Argument if self.current.is_empty() && word.is_group_brace() => {
                self.separator();
            }
            NextWord::Argument => {
                if self.current.is_empty() {
                    self.first_start.get_or_insert(self.word_start);
                }
                self.current.push(word.text);
            }
        }
    }

    /// Fecha a palavra pendente antes: o comando só termina depois da última palavra.
    fn end_command(&mut self) {
        self.end_word();
        if !self.current.is_empty() {
            self.commands.push(mem::take(&mut self.current));
        }
    }

    fn separator(&mut self) {
        self.end_command();
        self.separated = true;
    }

    /// A quebra de linha termina o comando mas não conta como separador; depois dela vêm os corpos de heredoc.
    fn line_break(&mut self) -> Result<(), ReadError> {
        self.end_command();
        self.executes_heredoc |= heredoc::skip_bodies(&mut self.cursor, &mut self.heredocs)?;

        Ok(())
    }

    fn ampersand(&mut self) -> Result<(), ReadError> {
        if self.cursor.eat('>') {
            return self.redirect('>');
        }
        self.separator();

        Ok(())
    }

    fn redirect(&mut self, operator: char) -> Result<(), ReadError> {
        if self.word.as_ref().is_some_and(WordBuilder::is_descriptor) {
            self.word = None;
        } else {
            self.end_word();
        }
        if self.cursor.eat('(') {
            self.separator();
            return self.nested(')');
        }
        self.next_word = redirect::after_operator(operator, &mut self.cursor);

        Ok(())
    }

    /// Lê uma substituição (`$(...)`, crase, `<(...)`) como comandos próprios; o comando e a palavra
    /// que a contêm ficam guardados e voltam depois.
    fn nested(&mut self, close: char) -> Result<(), ReadError> {
        let outer_word = self.word.take();
        let outer_start = self.word_start;
        let outer_command = mem::take(&mut self.current);
        let outer_next = mem::take(&mut self.next_word);
        self.separated = true;
        // O limite protege a pilha de um comando aninhado de propósito.
        if self.depth == MAX_DEPTH {
            return Err(ReadError::TooDeep);
        }
        self.depth += 1;
        let result = self.run(Some(close));
        self.depth -= 1;
        result?;
        self.word = outer_word;
        self.word_start = outer_start;
        self.current = outer_command;
        self.next_word = outer_next;

        Ok(())
    }

    /// `$(...)` e crase: além de ler por dentro, a palavra que os contém ganha um `$`. O leitor não sabe o que o
    /// shell vai expandir, então a palavra existe e diz que é expansão. O que vem colado depois (`$(true)#`)
    /// segue na mesma palavra, então o `#` ali não abre comentário.
    fn substituted(&mut self, close: char) -> Result<(), ReadError> {
        self.nested(close)?;
        self.push('$', false);

        Ok(())
    }

    fn dollar(&mut self) -> Result<(), ReadError> {
        let start = self.cursor.at - 1;
        if self.cursor.eat('(') {
            let (opens_arithmetic, scanned) = self.cursor.opens_arithmetic();
            self.scanned += scanned;
            if self.scanned > MAX_SCAN {
                return Err(ReadError::TooLong);
            }
            if !opens_arithmetic {
                return self.substituted(')');
            }
            self.cursor.eat('(');
            return self.expansion(start, Self::arithmetic);
        }
        if self.cursor.eat('{') {
            return self.expansion(start, Self::parameter);
        }
        // `$'...'` e `$"..."` são só aspas: o `$` não entra na palavra.
        if matches!(self.cursor.peek(), Some('\'' | '"')) {
            self.word.get_or_insert_default();
        } else {
            self.push('$', false);
        }

        Ok(())
    }

    /// `${...}` e `$((...))` ficam na palavra como texto cru, desde o `$` em `start`: o leitor não os expande,
    /// então a palavra tem de continuar existindo e dizer o que o bash vai ler. O que `scan` empurra por dentro é descartado.
    fn expansion(
        &mut self,
        start: usize,
        scan: fn(&mut Self) -> Result<(), ReadError>,
    ) -> Result<(), ReadError> {
        let outer_word = self.word.take().unwrap_or_default();
        if self.depth == MAX_DEPTH {
            return Err(ReadError::TooDeep);
        }
        self.depth += 1;
        let result = scan(self);
        self.depth -= 1;
        result?;
        self.word = Some(outer_word);
        for c in self.cursor.text_since(start).chars() {
            self.push(c, false);
        }

        Ok(())
    }

    /// `${...}` é parte da palavra, não separador: por dentro só importam `$(`, crase e aspas, e `#` não abre comentário.
    fn parameter(&mut self) -> Result<(), ReadError> {
        while let Some(c) = self.cursor.bump() {
            match c {
                '}' => break,
                '\\' => self.escape(),
                '\'' => self.single_quoted()?,
                '"' => self.double_quoted()?,
                _ => {
                    self.substitution(c)?;
                }
            }
        }

        Ok(())
    }

    /// `$((...))` é parte da palavra; só `$(` e crase por dentro executam.
    fn arithmetic(&mut self) -> Result<(), ReadError> {
        let mut depth = 0;
        while let Some(c) = self.cursor.bump() {
            match c {
                '(' => depth += 1,
                ')' if depth > 0 => depth -= 1,
                ')' => {
                    self.cursor.eat(')');
                    break;
                }
                _ => {
                    self.substitution(c)?;
                }
            }
        }

        Ok(())
    }

    /// `$` e crase abrem o que o shell executa; devolve se `c` era um deles.
    fn substitution(&mut self, c: char) -> Result<bool, ReadError> {
        match c {
            '$' => self.dollar()?,
            '`' => self.substituted('`')?,
            _ => return Ok(false),
        }

        Ok(true)
    }

    fn escape(&mut self) {
        // A barra antes da quebra de linha é continuação: não deixa nada na palavra.
        if let Some(c) = self.cursor.bump().filter(|c| *c != '\n') {
            self.push(c, true);
        }
    }

    fn single_quoted(&mut self) -> Result<(), ReadError> {
        self.word.get_or_insert_default().quoted = true;
        loop {
            match self.cursor.bump() {
                None => return Err(ReadError::UnclosedQuote),
                Some('\'') => return Ok(()),
                Some(c) => self.push(c, true),
            }
        }
    }

    /// Literal, menos `$(`, `${` e crase, que executam.
    fn double_quoted(&mut self) -> Result<(), ReadError> {
        self.word.get_or_insert_default().quoted = true;
        loop {
            match self.cursor.bump() {
                None => return Err(ReadError::UnclosedQuote),
                Some('"') => return Ok(()),
                Some('\\') => self.escape(),
                Some(c) => {
                    if !self.substitution(c)? {
                        self.push(c, true);
                    }
                }
            }
        }
    }
}
