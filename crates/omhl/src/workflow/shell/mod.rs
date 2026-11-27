//! Puro, sem IO e sem biblioteca de shell: aspas, heredocs, separadores, redirecionamentos e embrulhos.

mod cursor;
mod heredoc;
mod lexer;
mod redirect;
mod wrapper;

use self::wrapper::{Opacity, Resolved, basename};

const MAX_COMMAND_BYTES: usize = 256 * 1024;
const OMHL: &str = "omhl";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    UnclosedQuote,
    UnclosedHeredoc,
    TooDeep,
    TooLong,
}

#[derive(Debug)]
pub struct CommandLine {
    pub commands: Vec<SimpleCommand>,
    pub compound: bool,
    pub clean_start: bool,
    pub executes_heredoc: bool,
    pub mentions_omhl: bool,
}

#[derive(Debug)]
pub struct SimpleCommand {
    words: Vec<String>,
    resolved: Resolved,
}

impl SimpleCommand {
    pub fn program(&self) -> Option<&str> {
        self.resolved.program_at.map(|at| basename(&self.words[at]))
    }

    pub fn words(&self) -> &[String] {
        &self.words
    }

    pub fn split(&self) -> (&[String], Option<&str>, &[String]) {
        match self.resolved.program_at {
            Some(at) => (
                &self.words[..at],
                Some(basename(&self.words[at])),
                &self.words[at + 1..],
            ),
            None => (&self.words, None, &[]),
        }
    }

    pub fn follows_keyword(&self) -> bool {
        self.split().0.iter().any(|word| wrapper::is_keyword(word))
    }

    /// Roda o `omhl` ou o leitor não consegue descartar que rode: nega-se em vez de adivinhar.
    pub fn may_run_omhl(&self, mentions_omhl: bool) -> bool {
        match self.resolved.opacity {
            None if self.program().is_some_and(is_omhl_word) => true,
            // O programa é uma expansão (`$X`, `${...}`): o bash decide o que roda, o leitor não sabe.
            None => {
                self.resolved
                    .program_at
                    .is_some_and(|at| self.words[at].contains('$'))
                    && mentions_omhl
            }
            Some(Opacity::Rewriting) => mentions_omhl,
            Some(Opacity::UnknownOption) => {
                self.words.iter().any(|word| is_omhl_word(word))
                    || (mentions_omhl && self.words.iter().any(|word| word.contains('$')))
            }
        }
    }
}

/// O texto cru tem `omhl` em qualquer lugar; sensível a caixa.
pub fn mentions_omhl(text: &str) -> bool {
    text.contains(OMHL)
}

/// A palavra é o programa `omhl`, com ou sem diretório na frente.
pub fn is_omhl_word(word: &str) -> bool {
    basename(word) == OMHL
}

pub fn read_command(command: &str) -> Result<CommandLine, ReadError> {
    if command.len() > MAX_COMMAND_BYTES {
        return Err(ReadError::TooLong);
    }
    let lexed = lexer::lex(command)?;
    let compound = lexed.separated || lexed.commands.len() > 1;
    let leading_blanks = command
        .chars()
        .take_while(|c| matches!(c, ' ' | '\t'))
        .count();
    let clean_start = lexed
        .first_start
        .is_none_or(|start| start == leading_blanks);
    let commands = lexed
        .commands
        .into_iter()
        .map(|words| SimpleCommand {
            resolved: wrapper::resolve(&words),
            words,
        })
        .collect();

    Ok(CommandLine {
        commands,
        compound,
        clean_start,
        executes_heredoc: lexed.executes_heredoc,
        mentions_omhl: mentions_omhl(command),
    })
}

#[cfg(test)]
mod tests;
