use std::mem;

use super::ReadError;
use super::cursor::Cursor;
use super::heredoc::{self, Delimiter};
use super::redirect::{self, NextWord};

/// Os comandos simples na ordem de leitura (cada um é a lista das suas palavras, sem aspas) e se algum separador apareceu.
pub(super) struct Lexed {
    pub(super) commands: Vec<Vec<String>>,
    pub(super) separated: bool,
}

pub(super) fn lex(source: &str) -> Result<Lexed, ReadError> {
    let mut lexer = Lexer {
        cursor: Cursor::new(source),
        commands: Vec::new(),
        current: Vec::new(),
        word: None,
        next_word: NextWord::Argument,
        heredocs: Vec::new(),
        separated: false,
    };
    lexer.run(None)?;
    if !lexer.heredocs.is_empty() {
        return Err(ReadError::UnclosedHeredoc);
    }

    Ok(Lexed {
        commands: lexer.commands,
        separated: lexer.separated,
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
    next_word: NextWord,
    heredocs: Vec<Delimiter>,
    separated: bool,
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
            '`' => self.nested('`')?,
            '$' => self.dollar()?,
            '\\' => self.escape(),
            _ => self.push(c, false),
        }

        Ok(())
    }

    fn push(&mut self, c: char, quoted: bool) {
        let word = self.word.get_or_insert_default();
        word.text.push(c);
        word.quoted |= quoted;
    }

    /// Fecha a palavra em andamento conforme o que o operador anterior pediu.
    fn end_word(&mut self) {
        let Some(word) = self.word.take() else {
            return;
        };
        match mem::take(&mut self.next_word) {
            NextWord::Ignored => {}
            NextWord::Delimiter { strip_tabs } => self.heredocs.push(Delimiter {
                tag: word.text,
                strip_tabs,
            }),
            NextWord::Argument if self.current.is_empty() && word.is_group_brace() => {
                self.separator();
            }
            NextWord::Argument => self.current.push(word.text),
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

        heredoc::skip_bodies(&mut self.cursor, &mut self.heredocs)
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

    /// Lê uma substituição (`$(...)`, `${...}`, crase, `<(...)`) como comandos próprios; o comando e a
    /// palavra que a contêm ficam guardados e voltam depois.
    fn nested(&mut self, close: char) -> Result<(), ReadError> {
        let outer_word = self.word.take();
        let outer_command = mem::take(&mut self.current);
        let outer_next = mem::take(&mut self.next_word);
        self.separated = true;
        self.run(Some(close))?;
        self.word = outer_word;
        self.current = outer_command;
        self.next_word = outer_next;

        Ok(())
    }

    fn dollar(&mut self) -> Result<(), ReadError> {
        if self.cursor.eat('(') {
            self.nested(')')
        } else if self.cursor.eat('{') {
            self.nested('}')
        } else {
            self.push('$', false);
            Ok(())
        }
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
                Some('$') => self.dollar()?,
                Some('`') => self.nested('`')?,
                Some(c) => self.push(c, true),
            }
        }
    }
}
