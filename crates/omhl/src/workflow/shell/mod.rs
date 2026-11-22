//! Leitor de comando de shell do hook `agent-role`: só o que as regras do hook precisam saber.
//! Puro, sem IO e sem biblioteca de shell: aspas, heredocs, separadores, redirecionamentos e embrulhos.

mod cursor;
mod heredoc;
mod lexer;
mod redirect;
mod wrapper;

use self::wrapper::{Opacity, Resolved, basename};

/// Maior comando que o leitor aceita (256 KiB): acima disso ele não lê.
const MAX_COMMAND_BYTES: usize = 256 * 1024;

/// O leitor não sabe o que o comando faz: aspas ou heredoc sem fechar, aninhamento ou tamanho acima do limite.
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
    /// Mais de um comando simples, ou algum separador. Redirecionamento e a quebra de linha final não contam.
    pub compound: bool,
    /// Só espaços e tabs antes da primeira palavra do primeiro comando (sem comentário, linha em branco, redirecionamento ou continuação).
    pub clean_start: bool,
    /// Heredoc sem aspas no terminador cujo corpo tem `$(` ou crase: o bash executa o que está lá dentro.
    pub executes_heredoc: bool,
    /// O texto cru do comando inteiro tem `omhl`: `$(command -v omhl)` e `P=omhl; $P` também contam.
    pub mentions_omhl: bool,
}

/// Um comando simples: o trecho entre separadores, com as palavras sem aspas.
#[derive(Debug)]
pub struct SimpleCommand {
    words: Vec<String>,
    resolved: Resolved,
}

impl SimpleCommand {
    /// O programa real (basename) depois de pular atribuições e embrulhos; `None` se o leitor não o achou.
    pub fn program(&self) -> Option<&str> {
        self.resolved.program_at.map(|at| basename(&self.words[at]))
    }

    /// O comando em três partes: as palavras antes do programa (atribuições, embrulhos e suas opções), o programa real e as palavras depois dele.
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

    /// Uma palavra reservada (`!`, `if`, `do`...) antes do programa: ali um `VAR=valor` na frente não vale.
    pub fn follows_keyword(&self) -> bool {
        self.split().0.iter().any(|word| wrapper::is_keyword(word))
    }

    /// Roda o `omhl` ou o leitor não consegue descartar que rode: nega-se em vez de adivinhar.
    /// `mentions_omhl` vem do texto cru do comando inteiro (`CommandLine::mentions_omhl`).
    pub fn may_run_omhl(&self, mentions_omhl: bool) -> bool {
        match self.resolved.opacity {
            None if self.program() == Some("omhl") => true,
            // O programa é uma expansão (`$X`, `${...}`): o bash decide o que roda, o leitor não sabe.
            None => {
                self.resolved
                    .program_at
                    .is_some_and(|at| self.words[at].contains('$'))
                    && mentions_omhl
            }
            Some(Opacity::Rewriting) => mentions_omhl,
            Some(Opacity::UnknownOption) => {
                self.words.iter().any(|word| basename(word) == "omhl")
                    || (mentions_omhl && self.words.iter().any(|word| word.contains('$')))
            }
        }
    }
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
        mentions_omhl: command.contains("omhl"),
    })
}

#[cfg(test)]
mod tests;
