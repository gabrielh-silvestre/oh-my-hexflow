//! Texto do `omhl render`: o produto é o `text` do contexto sem controles, igual nos dois layouts.

use serde_json::Value;

use super::Renderer;
use super::base::{Entry, Profile, Region, Shape, Specialization};
use super::doc::{Closing, Doc, Line};
use super::error;

/// O `omitted` é ruído: o rodapé do próprio `text` já mostra quantos nós ficaram de fora.
pub static PROFILE: Profile = Profile {
    label: "render",
    nodes: None,
    success: &[
        Entry::new(Region::Root, "text", Shape::Closed),
        Entry::new(Region::Root, "omitted", Shape::Noise),
    ],
    failure: &error::ENTRIES,
};

pub struct Render;

/// Só o erro genérico usa a `Specialization`; o sucesso não tem cabeçalho nem resumo.
impl Specialization for Render {
    fn profile(&self) -> &'static Profile {
        &PROFILE
    }

    fn summary(&self, _value: &Value) -> Vec<String> {
        Vec::new()
    }
}

impl Renderer for Render {
    /// Uma linha crua com o `text`: o contexto é multilinha, então só `\n` e `\t` sobrevivem entre os controles.
    /// O JSON e o hook `session-start` seguem com o `text` inteiro.
    fn success(&self, value: &Value) -> Doc {
        Doc {
            lines: vec![Line::Raw {
                depth: 0,
                text: value["text"]
                    .as_str()
                    .unwrap_or_default()
                    .chars()
                    .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
                    .collect(),
            }],
            closing: Closing::None,
        }
    }

    fn failure(&self, value: &Value) -> Doc {
        error::failure(self, value)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn it_should_drop_controls_but_keep_line_breaks_and_tabs_in_the_context_text() {
        let value = json!({ "text": "a\n\tb\u{1b}]0;x\u{7}c\r\u{85}d" });

        let doc = Render.success(&value);

        assert_eq!(
            doc.lines,
            vec![Line::Raw {
                depth: 0,
                text: "a\n\tb]0;xcd".to_owned()
            }]
        );
    }
}
