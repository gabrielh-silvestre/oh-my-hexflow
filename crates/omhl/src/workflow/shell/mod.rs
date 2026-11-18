//! Leitor de comando de shell do hook `agent-role`: só o que as regras do hook precisam saber.
//! Puro, sem IO e sem biblioteca de shell: aspas, heredocs, separadores, redirecionamentos e embrulhos.

mod cursor;
mod heredoc;
mod lexer;
mod redirect;
mod wrapper;

pub use self::wrapper::Opacity;

use self::wrapper::{Resolved, basename};

/// Aspas ou heredoc sem fechar: o leitor não sabe o que o comando faz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    UnclosedQuote,
    UnclosedHeredoc,
}

/// O comando da ferramenta Bash como lista de comandos simples.
#[derive(Debug)]
pub struct CommandLine {
    pub commands: Vec<SimpleCommand>,
    /// Mais de um comando simples, ou algum separador. Redirecionamento e a quebra de linha final não contam.
    pub compound: bool,
}

/// Um comando simples: o trecho entre separadores, com as palavras sem aspas.
#[derive(Debug)]
pub struct SimpleCommand {
    words: Vec<String>,
    resolved: Resolved,
}

impl SimpleCommand {
    /// Todas as palavras, atribuições e embrulhos incluídos.
    pub fn words(&self) -> &[String] {
        &self.words
    }

    /// O programa real (basename) depois de pular atribuições e embrulhos; `None` se o leitor não o achou.
    pub fn program(&self) -> Option<&str> {
        self.resolved.program_at.map(|at| basename(&self.words[at]))
    }

    /// As palavras antes do programa: atribuições, embrulhos e suas opções.
    pub fn prefix(&self) -> &[String] {
        &self.words[..self.resolved.program_at.unwrap_or(self.words.len())]
    }

    /// As palavras depois do programa real.
    pub fn args(&self) -> &[String] {
        self.resolved
            .program_at
            .map_or(&[], |at| &self.words[at + 1..])
    }

    pub fn runs_omhl(&self) -> bool {
        self.program() == Some("omhl")
    }

    /// Roda o `omhl` ou o leitor não consegue descartar que rode: nega-se em vez de adivinhar.
    pub fn may_run_omhl(&self) -> bool {
        match self.resolved.opacity {
            None => self.runs_omhl(),
            Some(Opacity::Rewriting) => self.words.iter().any(|word| word.contains("omhl")),
            Some(Opacity::UnknownOption) => self.words.iter().any(|word| basename(word) == "omhl"),
        }
    }
}

pub fn read_command(command: &str) -> Result<CommandLine, ReadError> {
    let lexed = lexer::lex(command)?;
    let compound = lexed.separated || lexed.commands.len() > 1;
    let commands = lexed
        .commands
        .into_iter()
        .map(|words| SimpleCommand {
            resolved: wrapper::resolve(&words),
            words,
        })
        .collect();

    Ok(CommandLine { commands, compound })
}

#[cfg(test)]
mod tests {
    use super::{CommandLine, ReadError, read_command};

    fn read(command: &str) -> CommandLine {
        read_command(command).expect("the command is readable")
    }

    fn programs(command: &str) -> Vec<Option<String>> {
        read(command)
            .commands
            .iter()
            .map(|simple| simple.program().map(str::to_owned))
            .collect()
    }

    fn program_names(command: &str) -> Vec<&'static str> {
        programs(command)
            .into_iter()
            .map(|program| match program.as_deref() {
                Some("omhl") => "omhl",
                Some(_) => "other",
                None => "none",
            })
            .collect()
    }

    #[test]
    fn it_should_read_a_single_simple_command_as_not_compound() {
        let line = read("grep -rn omhl crates");

        assert_eq!(line.commands.len(), 1);
        assert!(!line.compound);
        assert_eq!(line.commands[0].program(), Some("grep"));
    }

    #[test]
    fn it_should_split_on_every_separator_of_the_list() {
        for command in [
            "a | b", "a || b", "a && b", "a ; b", "a & b", "a\nb", "a $(b)", "a `b`", "(a) b",
            "a <(b)", "a >(b)",
        ] {
            let line = read(command);

            assert_eq!(line.commands.len(), 2, "{command}");
            assert!(line.compound, "{command}");
        }
    }

    #[test]
    fn it_should_not_count_the_final_line_break_as_a_separator() {
        let line = read("omhl list\n");

        assert_eq!(line.commands.len(), 1);
        assert!(!line.compound);
    }

    #[test]
    fn it_should_not_treat_redirections_as_separators() {
        for command in [
            "omhl list > out",
            "omhl list >> out",
            "omhl list < in",
            "omhl list 2>&1",
            "omhl list >&2",
            "omhl list &> out",
            "omhl list <<< word",
            "omhl list 2>/dev/null",
        ] {
            let line = read(command);

            assert_eq!(line.commands.len(), 1, "{command}");
            assert!(!line.compound, "{command}");
            assert_eq!(line.commands[0].words(), ["omhl", "list"], "{command}");
        }
    }

    #[test]
    fn it_should_never_take_a_redirection_target_as_the_program() {
        assert_eq!(program_names("> omhl\nls"), ["other"]);
    }

    #[test]
    fn it_should_keep_a_separator_inside_quotes_as_data() {
        let line = read("grep 'a | b' \"c ; d\" e\\;f");

        assert_eq!(line.commands.len(), 1);
        assert!(!line.compound);
    }

    #[test]
    fn it_should_run_a_substitution_inside_double_quotes_but_not_inside_single_quotes() {
        assert_eq!(program_names("echo \"$(omhl list)\""), ["omhl", "other"]);
        assert_eq!(
            program_names("echo \"${x:-$(omhl list)}\""),
            ["omhl", "other", "other"]
        );
        assert_eq!(program_names("echo '$(omhl list)'"), ["other"]);
    }

    #[test]
    fn it_should_treat_a_heredoc_body_as_data() {
        let line = read("cat <<'EOF'\nomhl list; OMHL_ROLE=x\nEOF\n");

        assert_eq!(line.commands.len(), 1);
        assert!(!line.compound);
    }

    #[test]
    fn it_should_close_a_dash_heredoc_on_a_tab_indented_terminator() {
        let line = read("cat <<-EOF\n\tbody\n\tEOF\nomhl list");

        assert_eq!(line.commands.len(), 2);
    }

    #[test]
    fn it_should_not_take_a_here_string_for_a_heredoc() {
        assert_eq!(program_names("cat <<< word\nomhl list"), ["other", "omhl"]);
    }

    #[test]
    fn it_should_keep_the_commands_after_a_heredoc_operator_on_the_same_line() {
        assert_eq!(
            program_names("cat <<EOF; omhl list\nbody\nEOF\n"),
            ["other", "omhl"]
        );
    }

    #[test]
    fn it_should_report_an_unclosed_quote_or_heredoc() {
        assert_eq!(
            read_command("echo 'a").unwrap_err(),
            ReadError::UnclosedQuote
        );
        assert_eq!(
            read_command("echo \"a").unwrap_err(),
            ReadError::UnclosedQuote
        );
        assert_eq!(
            read_command("cat <<EOF\nbody").unwrap_err(),
            ReadError::UnclosedHeredoc
        );
        assert_eq!(
            read_command("cat <<EOF").unwrap_err(),
            ReadError::UnclosedHeredoc
        );
    }

    #[test]
    fn it_should_take_the_basename_of_the_program_and_skip_assignments() {
        assert_eq!(program_names("/home/x/.local/bin/omhl list"), ["omhl"]);
        assert_eq!(program_names("A=1 B=2 omhl list"), ["omhl"]);
        assert_eq!(read("A=1 omhl list").commands[0].args(), ["list"]);
    }

    #[test]
    fn it_should_skip_nested_wrappers_to_the_real_program() {
        for command in [
            "env A=1 omhl list",
            "time -p omhl list",
            "timeout 30 omhl list",
            "timeout -s KILL 30 omhl list",
            "nice -n 5 omhl list",
            "nice -n5 omhl list",
            "nohup omhl list",
            "command omhl list",
            "exec omhl list",
            "stdbuf -oL omhl list",
            "stdbuf -o L omhl list",
            "xargs -n 1 omhl list",
            "rtk omhl list",
            "rtk proxy omhl list",
            "rtk proxy env timeout 30 omhl list",
            "if omhl list; then",
        ] {
            let line = read(command);
            let found = line.commands.iter().find(|simple| simple.runs_omhl());

            assert_eq!(
                found.map(|simple| simple.args()),
                Some(["list".to_owned()].as_slice()),
                "{command}"
            );
        }
    }

    #[test]
    fn it_should_stop_at_a_rewriting_option_and_say_the_program_may_be_hidden() {
        for command in [
            "env -i omhl list",
            "env - omhl list",
            "env --ignore-environment omhl list",
            "env -S 'omhl list'",
            "exec -c omhl list",
        ] {
            let line = read(command);

            assert_eq!(line.commands[0].program(), None, "{command}");
            assert!(line.commands[0].may_run_omhl(), "{command}");
        }
    }

    #[test]
    fn it_should_stop_at_an_unknown_wrapper_option_and_only_flag_a_word_named_omhl() {
        assert!(read("nice --adjustment=5 omhl list").commands[0].may_run_omhl());
        assert!(!read("nice --frobnicate grep omhl-docs").commands[0].may_run_omhl());
    }

    #[test]
    fn it_should_read_a_brace_in_command_position_as_a_separator() {
        let line = read("{ omhl list\n}");

        assert_eq!(program_names("{ omhl list\n}"), ["omhl"]);
        assert!(line.compound);
        assert!(!read("echo {a,b} '{'").compound);
    }

    #[test]
    fn it_should_skip_a_comment_up_to_the_line_break() {
        assert_eq!(
            program_names("ls # it's omhl; omhl list\nls"),
            ["other", "other"]
        );
    }
}
