use std::borrow::Cow;

use regex::bytes::{Captures, Regex, RegexBuilder};

const SIZE_LIMIT: usize = 1 << 20;
const REDACTED: &[u8] = b"[REDACTED]";
const REDACTED_PREFIX: &[u8] = b"[REDACTED";
const PEM_BEGIN: &str = "-----BEGIN";
const PEM_MASK: &[u8] = b"[REDACTED:private-key]";
// Só o bloco PEM roda sobre o fluxo inteiro: ele atravessa linhas, os demais padrões não casam `\n`.
const PEM_PATTERN: &str = r"(?s-u)-----BEGIN [A-Z ]*PRIVATE KEY(?: BLOCK)?-----.*?(?:-----END [A-Z ]*PRIVATE KEY(?: BLOCK)?-----|\z)";

const ASSIGNMENT_KEYWORDS: &[&str] = &[
    "passw",
    "secret",
    "token",
    "api_key",
    "api-key",
    "apikey",
    "access_key",
    "access-key",
    "accesskey",
    "private_key",
    "private-key",
    "privatekey",
    "credential",
];

/// Como o trecho casado vira texto mascarado.
#[derive(Clone, Copy)]
enum Mask {
    /// Troca o trecho inteiro por um rótulo fixo.
    Label(&'static [u8]),
    /// Mantém o grupo 1 (prefixo) e mascara o resto.
    KeepPrefix,
    /// Mantém os grupos 1 e 2 e mascara a senha entre eles.
    UrlPassword,
    /// Atribuição `chave = valor`: grupo 1 é a fronteira, 2 a chave com o separador, 3 o valor entre aspas e 4 o terminador do valor sem aspas.
    Assignment,
}

struct RuleSpec {
    pattern: &'static str,
    triggers: &'static [&'static str],
    ignore_case: bool,
    mask: Mask,
}

// As nomeadas vêm antes das de contexto: a máscara de uma não pode ser lida como valor pela outra.
const RULES: &[RuleSpec] = &[
    RuleSpec {
        pattern: r"(?-u)\b(?:AKIA|ASIA|AGPA|AIDA|AROA|ANPA|ANVA)[0-9A-Z]{16}\b",
        triggers: &["AKIA", "ASIA", "AGPA", "AIDA", "AROA", "ANPA", "ANVA"],
        ignore_case: false,
        mask: Mask::Label(b"[REDACTED:aws-access-key]"),
    },
    RuleSpec {
        pattern: r"(?-u)\b(?:gh[pousr]_[A-Za-z0-9]{36,255}|github_pat_[A-Za-z0-9_]{22,255})\b",
        triggers: &["ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_"],
        ignore_case: false,
        mask: Mask::Label(b"[REDACTED:github-token]"),
    },
    RuleSpec {
        pattern: r"(?-u)\bglpat-[A-Za-z0-9_-]{20,}\b",
        triggers: &["glpat-"],
        ignore_case: false,
        mask: Mask::Label(b"[REDACTED:gitlab-token]"),
    },
    RuleSpec {
        pattern: r"(?-u)\bnpm_[A-Za-z0-9]{36}\b",
        triggers: &["npm_"],
        ignore_case: false,
        mask: Mask::Label(b"[REDACTED:npm-token]"),
    },
    RuleSpec {
        pattern: r"(?-u)\b(?:sk|rk)_(?:live|test)_[A-Za-z0-9]{16,}\b",
        triggers: &["sk_live_", "sk_test_", "rk_live_", "rk_test_"],
        ignore_case: false,
        mask: Mask::Label(b"[REDACTED:stripe-key]"),
    },
    RuleSpec {
        pattern: r"(?-u)\bxox[abprs]-[A-Za-z0-9-]{10,}\b",
        triggers: &["xox"],
        ignore_case: false,
        mask: Mask::Label(b"[REDACTED:slack-token]"),
    },
    RuleSpec {
        pattern: r"(?-u)https://hooks\.slack\.com/services/[A-Za-z0-9/_-]+",
        triggers: &["hooks.slack.com"],
        ignore_case: false,
        mask: Mask::Label(b"[REDACTED:slack-webhook]"),
    },
    RuleSpec {
        pattern: r"(?-u)\bsk-(?:ant-)?[A-Za-z0-9_-]{20,}\b",
        triggers: &["sk-"],
        ignore_case: false,
        mask: Mask::Label(b"[REDACTED:api-key]"),
    },
    RuleSpec {
        pattern: r"(?-u)\bAIza[0-9A-Za-z_-]{35}\b",
        triggers: &["AIza"],
        ignore_case: false,
        mask: Mask::Label(b"[REDACTED:google-api-key]"),
    },
    RuleSpec {
        pattern: r"(?-u)\beyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b",
        triggers: &["eyJ"],
        ignore_case: false,
        mask: Mask::Label(b"[REDACTED:jwt]"),
    },
    RuleSpec {
        pattern: r"(?i-u)\b(authorization[ \t]*[:=][ \t]*(?:bearer|basic|token)[ \t]+)[A-Za-z0-9._~+/=-]{8,}",
        triggers: &["authorization"],
        ignore_case: true,
        mask: Mask::KeepPrefix,
    },
    RuleSpec {
        pattern: r"(?i-u)\b(bearer[ \t]+)[A-Za-z0-9._~+/=-]{20,}",
        triggers: &["bearer"],
        ignore_case: true,
        mask: Mask::KeepPrefix,
    },
    RuleSpec {
        pattern: r"(?i-u)\b([a-z][a-z0-9+.-]*://[^\s/:@]+:)[^\s/@]+(@)",
        triggers: &["://"],
        ignore_case: false,
        mask: Mask::UrlPassword,
    },
    RuleSpec {
        pattern: concat!(
            r"(?i-u)",
            r#"(^|[ \t"'{(\[,;?&])(["']?[A-Za-z0-9_-]*(?:password|passwd|secret|token|api[_-]?key|access[_-]?key|private[_-]?key|credentials?)["']?[ \t]*=[ \t]*)"#,
            r#"(?:("[^"]{6,}"|'[^']{6,}')|[^\s"',;:()][^\s"',;()]{5,}([ \t"',;\r]|$))"#
        ),
        triggers: ASSIGNMENT_KEYWORDS,
        ignore_case: true,
        mask: Mask::Assignment,
    },
    RuleSpec {
        pattern: concat!(
            r"(?i-u)",
            r#"(^|[ \t"'{(\[,;?&])(["']?[A-Za-z0-9_-]*(?:password|passwd|secret|token|api[_-]?key|access[_-]?key|private[_-]?key|credentials?)["']?[ \t]*:[ \t]*)"#,
            r#"("[^"]{6,}"|'[^']{6,}')"#
        ),
        triggers: ASSIGNMENT_KEYWORDS,
        ignore_case: true,
        mask: Mask::Assignment,
    },
];

impl Mask {
    /// Escreve o trecho mascarado e devolve quantos bytes finais do casamento ficam para a próxima busca (o terminador, que também serve de fronteira ao par seguinte).
    fn write(self, found: &Captures<'_>, masked: &mut Vec<u8>) -> usize {
        match self {
            Mask::Label(label) => masked.extend_from_slice(label),
            Mask::KeepPrefix => {
                masked.extend_from_slice(&found[1]);
                masked.extend_from_slice(REDACTED);
            }
            Mask::UrlPassword => {
                masked.extend_from_slice(&found[1]);
                masked.extend_from_slice(REDACTED);
                masked.extend_from_slice(&found[2]);
            }
            Mask::Assignment => return write_assignment(found, masked),
        }

        0
    }
}

fn write_assignment(found: &Captures<'_>, masked: &mut Vec<u8>) -> usize {
    let whole = &found[0];
    let (value, terminator) = match found.get(3) {
        Some(quoted) => (&quoted.as_bytes()[1..], &whole[whole.len()..]),
        None => {
            let terminator = &found[4];
            let head = found[1].len() + found[2].len();
            (&whole[head..whole.len() - terminator.len()], terminator)
        }
    };
    let kept = &whole[..whole.len() - terminator.len()];
    // Valor já mascarado fica como está: a redação é idempotente.
    if value.starts_with(REDACTED_PREFIX) {
        masked.extend_from_slice(kept);
    } else {
        masked.extend_from_slice(&found[1]);
        masked.extend_from_slice(&found[2]);
        masked.extend_from_slice(REDACTED);
    }

    terminator.len()
}

struct Rule {
    regex: Regex,
    mask: Mask,
}

impl Rule {
    /// `None` quando nada casa.
    fn apply(&self, line: &[u8]) -> Option<Vec<u8>> {
        let mut masked = Vec::new();
        let (mut copied, mut search_from, mut matched) = (0, 0, false);
        while let Some(found) = self.regex.captures_at(line, search_from) {
            let Some(whole) = found.get(0) else { break };
            matched = true;
            masked.extend_from_slice(&line[copied..whole.start()]);
            let kept = self.mask.write(&found, &mut masked);
            copied = whole.end() - kept;
            search_from = copied;
        }
        if !matched {
            return None;
        }
        masked.extend_from_slice(&line[copied..]);

        Some(masked)
    }
}

/// Palavra que tem de aparecer na linha para a regra poder casar.
struct Trigger {
    literal: &'static [u8],
    ignore_case: bool,
    rule: u32,
}

impl Trigger {
    fn starts(&self, text: &[u8]) -> bool {
        let Some(head) = text.get(..self.literal.len()) else {
            return false;
        };

        if self.ignore_case {
            head.eq_ignore_ascii_case(self.literal)
        } else {
            head == self.literal
        }
    }
}

/// Posições onde alguma palavra-chave começa, em ordem, inclusive as que se sobrepõem.
struct TriggerStarts<'r, 's> {
    scan: &'r Regex,
    stream: &'s [u8],
    from: usize,
}

impl Iterator for TriggerStarts<'_, '_> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        let start = self.scan.find_at(self.stream, self.from)?.start();
        // Recomeça logo depois do início, não do fim: uma palavra pode começar dentro de outra.
        self.from = start + 1;

        Some(start)
    }
}

/// Mascara segredos da saída dos checks por uma lista de padrões que nunca cobre todo formato.
pub struct Redactor {
    pem: Regex,
    pem_begin: Regex,
    rules: Vec<Rule>,
    // Um passe sobre o fluxo inteiro acha onde começa qualquer palavra-chave; a de cada posição é lida pelo primeiro byte.
    trigger_scan: Regex,
    triggers_by_first_byte: Vec<Vec<Trigger>>,
    prefilter: bool,
}

impl Default for Redactor {
    fn default() -> Redactor {
        Redactor::new()
    }
}

impl Redactor {
    pub fn new() -> Redactor {
        let mut triggers_by_first_byte: Vec<Vec<Trigger>> =
            (0..=u8::MAX).map(|_| Vec::new()).collect();
        for (index, spec) in (0u32..).zip(RULES) {
            for text in spec.triggers {
                let literal = text.as_bytes();
                let first = literal[0];
                let mut starts = vec![first];
                if spec.ignore_case {
                    starts.push(first.to_ascii_uppercase());
                }
                for start in starts {
                    triggers_by_first_byte[usize::from(start)].push(Trigger {
                        literal,
                        ignore_case: spec.ignore_case,
                        rule: index,
                    });
                }
            }
        }

        let trigger_pattern = RULES
            .iter()
            .flat_map(|spec| {
                spec.triggers.iter().map(|text| {
                    let flags = if spec.ignore_case { "i" } else { "" };
                    format!("(?{flags}:{})", regex::escape(text))
                })
            })
            .collect::<Vec<_>>()
            .join("|");

        Redactor {
            pem: compile(PEM_PATTERN),
            pem_begin: compile(PEM_BEGIN),
            trigger_scan: compile(&format!("(?-u){trigger_pattern}")),
            rules: RULES
                .iter()
                .map(|spec| Rule {
                    regex: compile(spec.pattern),
                    mask: spec.mask,
                })
                .collect(),
            triggers_by_first_byte,
            prefilter: true,
        }
    }

    /// Mascara um fluxo inteiro; o trecho sem segredo sai byte a byte igual.
    pub fn redact(&self, stream: &[u8]) -> Vec<u8> {
        let stream = self.mask_private_keys(stream);
        let mut starts = TriggerStarts {
            scan: &self.trigger_scan,
            stream: &stream,
            from: 0,
        }
        .peekable();
        let mut masked = Vec::with_capacity(stream.len());
        let mut line_start = 0;
        for (index, line) in stream.split(|byte| *byte == b'\n').enumerate() {
            if index > 0 {
                masked.push(b'\n');
            }
            let line_end = line_start + line.len();
            let mut hits = 0;
            while let Some(start) = starts.next_if(|start| *start < line_end) {
                hits |= self.rules_starting_at(&stream[start..]);
            }
            masked.extend_from_slice(&self.mask_line(line, hits));
            line_start = line_end + 1;
        }

        masked
    }

    fn mask_private_keys<'s>(&self, stream: &'s [u8]) -> Cow<'s, [u8]> {
        if self.prefilter && !self.pem_begin.is_match(stream) {
            return Cow::Borrowed(stream);
        }

        self.pem.replace_all(stream, PEM_MASK)
    }

    fn mask_line<'l>(&self, line: &'l [u8], hits: u32) -> Cow<'l, [u8]> {
        let hits = if self.prefilter { hits } else { u32::MAX };
        let mut masked = Cow::Borrowed(line);
        for (index, rule) in (0u32..).zip(&self.rules) {
            if hits & (1 << index) == 0 {
                continue;
            }
            if let Some(next) = rule.apply(&masked) {
                masked = Cow::Owned(next);
            }
        }

        masked
    }

    /// Máscara de bits das regras cuja palavra começa no início de `text`.
    fn rules_starting_at(&self, text: &[u8]) -> u32 {
        let mut hits = 0;
        for trigger in &self.triggers_by_first_byte[usize::from(text[0])] {
            if trigger.starts(text) {
                hits |= 1 << trigger.rule;
            }
        }

        hits
    }
}

fn compile(pattern: &str) -> Regex {
    RegexBuilder::new(pattern)
        .size_limit(SIZE_LIMIT)
        .build()
        .expect("the built-in redaction patterns compile")
}

#[cfg(test)]
mod tests;
