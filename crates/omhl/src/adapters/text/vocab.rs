//! Vocabulário central da saída: o único lugar com símbolos, marcadores, separadores, recuo, `Next:`, o aviso de
//! segredos e as palavras que mais de um comando repete. `Vocabulary` é a base; `AgentVocabulary` e `HumanVocabulary`
//! a implementam e sobrescrevem só o que difere. Frases que um comando só usa ficam no comando.

use std::fmt::Display;

use super::doc::Join;

/// O estado que o `Doc` carrega no lugar do glifo: quem renderiza diz o estado e o vocabulário do layout escolhe o símbolo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Ok,
    Warning,
    Error,
}

pub const SYMBOL_OK: &str = "✔";
pub const SYMBOL_WARNING: &str = "⚠";
pub const SYMBOL_ERROR: &str = "✘";
pub const NEXT_LABEL: &str = "Next:";
pub const NOTHING_BLOCKED: &str = "Nothing blocked.";
pub const SECRET_WARNING: &str = "The output may contain secrets: do not copy it into a PR, issue, commit, --note or omhl append, and do not repeat it to the user.";

/// Caracteres que o texto mostra de um hash de snapshot ou de commit; o `Profile` e o corte do texto usam este valor.
pub const HASH_PREFIX_LEN: usize = 7;

pub const BLOCKING: &str = "blocking";
pub const NON_BLOCKING: &str = "non-blocking";
pub const TIMED_OUT: &str = "timed out";
pub const TRUNCATED: &str = "truncated";
pub const NONE: &str = "none";
pub const UNCHANGED: &str = "unchanged";
pub const WARNING: &str = "warning";

/// `seq 7`.
pub fn seq_label(seq: impl Display) -> String {
    format!("seq {seq}")
}

/// "N nouns", com o singular em 1.
pub fn plural(count: usize, noun: &str) -> String {
    match count {
        1 => format!("1 {noun}"),
        total => format!("{total} {noun}s"),
    }
}

/// `none` quando vazio; senão `plural`.
pub fn counted(count: usize, noun: &str) -> String {
    match count {
        0 => NONE.to_owned(),
        total => plural(total, noun),
    }
}

/// O fecho `Next:` acrescenta o ponto final; a dica que já o traz perde o dela.
pub fn without_final_period(hint: &str) -> String {
    hint.trim_end_matches('.').to_owned()
}

pub trait Vocabulary {
    fn symbol(&self, state: State) -> &'static str {
        match state {
            State::Ok => SYMBOL_OK,
            State::Warning => SYMBOL_WARNING,
            State::Error => SYMBOL_ERROR,
        }
    }

    fn dash(&self) -> &'static str {
        "- "
    }

    fn separator(&self, join: Join) -> &'static str {
        match join {
            Join::Dot => " · ",
            Join::Gap => "  ",
        }
    }

    fn indent(&self, depth: usize) -> String {
        "  ".repeat(depth)
    }

    fn next(&self, step: &str) -> String {
        format!("{NEXT_LABEL} {step}.")
    }

    fn nothing_blocked(&self, step: &str) -> String {
        format!("{NOTHING_BLOCKED} {}", self.next(step))
    }

    /// Limpeza de um valor (parte de cabeçalho ou de item, valor de campo, fato, nota, passo do fecho).
    fn clean(&self, value: &str) -> String {
        value.to_owned()
    }

    /// Limpeza de uma linha de saída de comando, que carrega o alinhamento do programa.
    fn clean_output(&self, line: &str) -> String {
        line.to_owned()
    }
}

/// Texto legível: mantém o espaçamento como o produto escreve.
pub struct HumanVocabulary;

impl Vocabulary for HumanVocabulary {}

/// Texto compacto: um só espaço entre as partes, sem espaço repetido nem nas pontas dos valores.
pub struct AgentVocabulary;

impl Vocabulary for AgentVocabulary {
    fn separator(&self, join: Join) -> &'static str {
        match join {
            Join::Gap => " ",
            Join::Dot => HumanVocabulary.separator(Join::Dot),
        }
    }

    fn clean(&self, value: &str) -> String {
        let mut cleaned = String::with_capacity(value.len());
        for character in value.chars() {
            if character == ' ' && (cleaned.is_empty() || cleaned.ends_with(' ')) {
                continue;
            }
            cleaned.push(character);
        }
        cleaned.truncate(cleaned.trim_end_matches(' ').len());

        cleaned
    }

    /// Só o fim da linha: o interior da saída literal de um programa pode ter alinhamento que carrega informação.
    fn clean_output(&self, line: &str) -> String {
        line.trim_end().to_owned()
    }
}

#[cfg(test)]
mod tests;
