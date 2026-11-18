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

/// O comando da ferramenta Bash como lista de comandos simples.
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
mod tests {
    use super::{CommandLine, MAX_COMMAND_BYTES, ReadError, lexer, read_command};

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
            assert_eq!(line.commands[0].words, ["omhl", "list"], "{command}");
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
            ["omhl", "other"]
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
        assert_eq!(read("A=1 omhl list").commands[0].split().2, ["list"]);
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
            let found = line
                .commands
                .iter()
                .find(|simple| simple.program() == Some("omhl"));

            assert_eq!(
                found.map(|simple| simple.split().2),
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
            assert!(
                line.commands[0].may_run_omhl(line.mentions_omhl),
                "{command}"
            );
        }
    }

    #[test]
    fn it_should_stop_at_an_unknown_wrapper_option_and_only_flag_a_word_named_omhl() {
        let flagged = read("nice --adjustment=5 omhl list");
        let docs = read("nice --frobnicate grep omhl-docs");

        assert!(flagged.commands[0].may_run_omhl(flagged.mentions_omhl));
        assert!(!docs.commands[0].may_run_omhl(docs.mentions_omhl));
    }

    #[test]
    fn it_should_read_a_brace_in_command_position_as_a_separator() {
        let line = read("{ omhl list\n}");

        assert_eq!(program_names("{ omhl list\n}"), ["omhl"]);
        assert!(line.compound);
        assert!(!read("echo {a,b} '{'").compound);
    }

    #[test]
    fn it_should_keep_the_raw_text_of_an_expansion_in_its_word() {
        for (command, words) in [
            (
                "omhl ${X:-attest} list",
                vec!["omhl", "${X:-attest}", "list"],
            ),
            ("omhl $((0)) list", vec!["omhl", "$((0))", "list"]),
            ("echo \"${X}\"", vec!["echo", "${X}"]),
            ("echo a${x:-'b'}c", vec!["echo", "a${x:-'b'}c"]),
        ] {
            assert_eq!(read(command).commands[0].words, words, "{command}");
        }
    }

    #[test]
    fn it_should_read_up_to_the_depth_limit_and_refuse_one_level_more() {
        for (opening, closing) in [
            ("$(", ")"),
            ("${a:-", "}"),
            ("$((", "))"),
            ("(", ")"),
            ("<(", ")"),
        ] {
            let nested =
                |depth: usize| format!("{}x{}", opening.repeat(depth), closing.repeat(depth));

            assert!(read_command(&nested(lexer::MAX_DEPTH)).is_ok(), "{opening}");
            assert_eq!(
                read_command(&nested(lexer::MAX_DEPTH + 1)).unwrap_err(),
                ReadError::TooDeep,
                "{opening}"
            );
        }
    }

    #[test]
    fn it_should_read_up_to_the_length_limit_and_refuse_one_byte_more() {
        assert!(read_command(&"a".repeat(MAX_COMMAND_BYTES)).is_ok());
        assert_eq!(
            read_command(&"a".repeat(MAX_COMMAND_BYTES + 1)).unwrap_err(),
            ReadError::TooLong
        );
    }

    #[test]
    fn it_should_count_the_length_limit_in_bytes_not_characters() {
        assert!(read_command(&"ã".repeat(MAX_COMMAND_BYTES / 2)).is_ok());
        assert_eq!(
            read_command(&"ã".repeat(MAX_COMMAND_BYTES / 2 + 1)).unwrap_err(),
            ReadError::TooLong
        );
    }

    #[test]
    fn it_should_refuse_a_command_whose_arithmetic_scans_add_up_to_quadratic_work() {
        // Cada `$((` olha o resto do texto; as aspas escondem o `))` de fora e a varredura só para no fim.
        let command = "$((\"((\")) ".repeat(MAX_COMMAND_BYTES / 10);

        assert_eq!(read_command(&command).unwrap_err(), ReadError::TooLong);
    }

    #[test]
    fn it_should_keep_a_word_for_a_substitution_and_flag_it_only_when_the_command_mentions_omhl() {
        let line = read("$(which omhl) list");
        let outer = &line.commands[1];

        assert_eq!(outer.words, ["$", "list"]);
        assert!(outer.may_run_omhl(true));
        assert!(!outer.may_run_omhl(false));
    }

    #[test]
    fn it_should_flag_an_unknown_wrapper_option_before_an_expansion_when_omhl_is_mentioned() {
        let line = read("nice --frobnicate $(which omhl) list");
        let outer = &line.commands[1];

        assert!(line.mentions_omhl);
        assert!(outer.may_run_omhl(true));
        assert!(!outer.may_run_omhl(false));
    }

    #[test]
    fn it_should_not_run_the_body_of_a_heredoc_whose_terminator_is_quoted_in_any_way() {
        for command in [
            "cat <<'EOF'\n$(date)\nEOF\n",
            "cat <<\"EOF\"\n$(date)\nEOF\n",
            "cat <<\\EOF\n$(date)\nEOF\n",
        ] {
            assert!(!read(command).executes_heredoc, "{command}");
        }
    }

    #[test]
    fn it_should_run_the_body_of_an_unquoted_heredoc_even_when_the_dollar_is_escaped() {
        // Falso positivo seguro: a barra deixa o `$(date)` literal, mas o leitor não distingue e nega.
        assert!(read("cat <<EOF\n\\$(date)\nEOF\n").executes_heredoc);
    }

    #[test]
    fn it_should_read_two_heredocs_on_one_line_as_one_command() {
        let line = read("cat <<A <<B\na\nA\nb\nB\n");

        assert_eq!(line.commands.len(), 1);
        assert!(!line.executes_heredoc);
    }

    #[test]
    fn it_should_read_a_quoted_heredoc_inside_a_substitution_as_data() {
        let command = "echo \"$(cat <<'EOF'\n$(omhl list)\nEOF\n)\"";

        assert_eq!(program_names(command), ["other", "other"]);
        assert!(!read(command).executes_heredoc);
    }

    #[test]
    fn it_should_skip_a_comment_up_to_the_line_break() {
        assert_eq!(
            program_names("ls # it's omhl; omhl list\nls"),
            ["other", "other"]
        );
    }
}
