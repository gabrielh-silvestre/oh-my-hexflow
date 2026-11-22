#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Opacity {
    Rewriting,
    UnknownOption,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Resolved {
    pub(super) program_at: Option<usize>,
    pub(super) opacity: Option<Opacity>,
}

/// As opções que sabe pular de forma determinística.
struct Wrapper {
    name: &'static str,
    flags: &'static [&'static str],
    valued: &'static [&'static str],
    rewriting: &'static [&'static str],
    lookups: &'static [&'static str],
    /// Palavras posicionais antes do comando embrulhado (a duração do `timeout`).
    positionals: usize,
    /// Subcomando opcional entre o embrulho e o comando (`rtk proxy`).
    subcommand: Option<&'static str>,
}

const PLAIN: Wrapper = Wrapper {
    name: "",
    flags: &[],
    valued: &[],
    rewriting: &[],
    lookups: &[],
    positionals: 0,
    subcommand: None,
};

/// Palavras reservadas do shell que antecedem um comando (`if omhl ...; then omhl ...`).
const KEYWORDS: [&str; 8] = ["if", "then", "elif", "else", "while", "until", "do", "!"];

const NO_PROGRAM: Resolved = Resolved {
    program_at: None,
    opacity: None,
};

/// Igualdade exata: o bash só lê palavra reservada nua, então `./do` e `FOO=/srv/do` não são palavras reservadas.
pub(super) fn is_keyword(word: &str) -> bool {
    KEYWORDS.contains(&word)
}

const WRAPPERS: [Wrapper; 10] = [
    Wrapper {
        name: "env",
        valued: &["-u", "--unset"],
        rewriting: &["-", "-i", "-S", "--ignore-environment", "--split-string"],
        ..PLAIN
    },
    Wrapper {
        name: "time",
        flags: &["-p"],
        ..PLAIN
    },
    Wrapper {
        name: "timeout",
        flags: &["--foreground", "--preserve-status", "-v", "--verbose"],
        valued: &["-s", "--signal", "-k", "--kill-after"],
        positionals: 1,
        ..PLAIN
    },
    Wrapper {
        name: "nice",
        valued: &["-n"],
        ..PLAIN
    },
    Wrapper {
        name: "nohup",
        ..PLAIN
    },
    Wrapper {
        name: "command",
        flags: &["-p"],
        lookups: &["-v", "-V"],
        ..PLAIN
    },
    Wrapper {
        name: "exec",
        flags: &["-l"],
        valued: &["-a"],
        rewriting: &["-c"],
        ..PLAIN
    },
    Wrapper {
        name: "stdbuf",
        valued: &["-i", "-o", "-e", "--input", "--output", "--error"],
        ..PLAIN
    },
    Wrapper {
        name: "xargs",
        flags: &[
            "-0",
            "-r",
            "-t",
            "-p",
            "-x",
            "--null",
            "--no-run-if-empty",
            "--verbose",
        ],
        valued: &[
            "-n",
            "-P",
            "-I",
            "-L",
            "-s",
            "-d",
            "-E",
            "--max-args",
            "--max-procs",
            "--delimiter",
        ],
        ..PLAIN
    },
    Wrapper {
        name: "rtk",
        subcommand: Some("proxy"),
        ..PLAIN
    },
];

enum Skipped {
    Words(usize),
    Opaque(Opacity),
    Lookup,
}

pub(super) fn basename(word: &str) -> &str {
    word.rsplit('/').next().unwrap_or(word)
}

fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        let name = name.strip_suffix('+').unwrap_or(name);

        !name.is_empty()
            && !name.starts_with(|c: char| c.is_ascii_digit())
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

pub(super) fn resolve(words: &[String]) -> Resolved {
    let mut at = 0;
    loop {
        at += words[at..].iter().take_while(|w| is_assignment(w)).count();
        let Some(word) = words.get(at) else {
            return NO_PROGRAM;
        };
        if is_keyword(word) {
            at += 1;
            continue;
        }
        let name = basename(word);
        let Some(wrapper) = WRAPPERS.iter().find(|wrapper| wrapper.name == name) else {
            return Resolved {
                program_at: Some(at),
                opacity: None,
            };
        };
        match wrapper.skip(&words[at + 1..]) {
            Skipped::Words(count) => at += 1 + count,
            Skipped::Lookup => return NO_PROGRAM,
            Skipped::Opaque(opacity) => {
                return Resolved {
                    program_at: None,
                    opacity: Some(opacity),
                };
            }
        }
    }
}

impl Wrapper {
    fn skip(&self, args: &[String]) -> Skipped {
        let mut at = 0;
        while let Some(word) = args.get(at).filter(|word| word.starts_with('-')) {
            if word == "--" {
                at += 1;
                break;
            }
            if self.lookups.contains(&word.as_str()) {
                return Skipped::Lookup;
            }
            if self.is_rewriting(word) {
                return Skipped::Opaque(Opacity::Rewriting);
            }
            let Some(width) = self.option_width(word) else {
                return Skipped::Opaque(Opacity::UnknownOption);
            };
            at += width;
        }
        at += self.positionals;
        if self
            .subcommand
            .is_some_and(|name| args.get(at).is_some_and(|word| word == name))
        {
            at += 1;
        }

        Skipped::Words(at.min(args.len()))
    }

    fn is_rewriting(&self, word: &str) -> bool {
        self.rewriting.iter().any(|option| {
            if option.len() == 1 {
                word == *option
            } else {
                word.starts_with(option)
            }
        })
    }

    fn option_width(&self, word: &str) -> Option<usize> {
        if self.flags.contains(&word) {
            return Some(1);
        }
        if self.valued.contains(&word) {
            return Some(2);
        }
        let attached = match word.split_once('=') {
            Some((long, _)) => self.valued.contains(&long),
            None => self
                .valued
                .iter()
                .any(|option| option.len() == 2 && word.starts_with(option)),
        };

        attached.then_some(1)
    }
}
