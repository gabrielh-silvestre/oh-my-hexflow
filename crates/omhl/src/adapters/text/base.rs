//! Esqueleto dos comandos de base: o `BaseRenderer` monta o `Doc` por Template Method e o `Profile` declara a forma de cada chave do `Value`.

use serde_json::Value;

use super::Renderer;
use super::doc::{Closing, Doc, Line};
use super::error::{self, ErrorFacts};
use super::vocab::State;

/// Onde a chave mora no nó (ou no `Value`, nos comandos sem nó): o mesmo nome vive em regiões diferentes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    /// Escalares na raiz do nó e, nos comandos sem nó, na raiz do `Value`.
    Root,
    /// Dentro do objeto `snapshot` da raiz.
    Snapshot,
    /// Dentro do objeto `author` da raiz.
    Author,
    /// Tudo sob `data` e, nos comandos sem nó, tudo sob objeto aninhado da raiz.
    Data,
}

/// Para qual tipo de nó a entrada vale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Any,
    NodeType(&'static str),
}

/// Como o texto mostra o escalar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// Valor que a fixture escolhe livremente: id, título, texto, nome.
    Literal,
    /// Vocabulário fechado ou texto que o app escreve ou compõe.
    Closed,
    /// Só hash de snapshot ou de commit: aparece pelos `n` primeiros caracteres. Ids nunca são encurtados.
    Hash(usize),
    /// O texto mostra o dado por outra forma (contagem, símbolo, duração).
    Derived,
    /// O texto não mostra o dado.
    Noise,
}

/// Qual dos dois desfechos da tabela vale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Success,
    Failure,
}

/// Onde estão os nós no `Value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeContainer {
    /// Array de nós sob a chave (`items` do `list`).
    Array(&'static str),
    /// Um nó sob a chave (`node` do `get`).
    Object(&'static str),
}

/// A chave `"*"` casa qualquer outra chave, só na região `Data`.
pub const WILDCARD: &str = "*";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub region: Region,
    pub scope: Scope,
    /// Chave do objeto ou array imediato (array é transparente); nunca mais de um nível.
    pub parent: Option<&'static str>,
    pub key: &'static str,
    pub shape: Shape,
    /// A chave pode faltar no estado: toda chave que não está em todos os estados do desfecho é opcional.
    pub optional: bool,
}

impl Entry {
    pub const fn new(region: Region, key: &'static str, shape: Shape) -> Entry {
        Entry {
            region,
            scope: Scope::Any,
            parent: None,
            key,
            shape,
            optional: false,
        }
    }

    /// Chave de `data` de um tipo de nó.
    pub const fn data(node_type: &'static str, key: &'static str, shape: Shape) -> Entry {
        Entry::new(Region::Data, key, shape).scoped(node_type)
    }

    pub const fn optional(self) -> Entry {
        Entry {
            optional: true,
            ..self
        }
    }

    pub const fn under(self, parent: &'static str) -> Entry {
        Entry {
            parent: Some(parent),
            ..self
        }
    }

    pub const fn scoped(self, node_type: &'static str) -> Entry {
        Entry {
            scope: Scope::NodeType(node_type),
            ..self
        }
    }

    fn matches(&self, region: Region, node_type: Option<&str>, key: &str) -> bool {
        let in_scope = match self.scope {
            Scope::Any => true,
            Scope::NodeType(wanted) => node_type == Some(wanted),
        };
        let same_key = self.key == key || (self.key == WILDCARD && region == Region::Data);

        self.region == region && in_scope && same_key
    }

    /// Ordem de precedência (menor vence): explícita com `parent` igual, explícita sem `parent`, curinga do tipo,
    /// curinga qualquer; empatando, o escopo `NodeType` vence o `Any`.
    fn rank(&self, parent: Option<&str>) -> Option<(u8, bool)> {
        let any_scope = self.scope == Scope::Any;
        match (self.key == WILDCARD, self.parent) {
            (true, _) => Some((2, any_scope)),
            (false, None) => Some((1, any_scope)),
            (false, Some(wanted)) if parent == Some(wanted) => Some((0, any_scope)),
            (false, Some(_)) => None,
        }
    }
}

/// Tabela de formas de um comando: o que o teste de folhas cobra do texto e o que o `BaseRenderer` tem de mostrar.
#[derive(Debug)]
pub struct Profile {
    /// Rótulo do comando no cabeçalho; vazio quando o cabeçalho abre direto pelo resumo (o `check run`).
    pub label: &'static str,
    /// `None` nos comandos sem nó.
    pub nodes: Option<NodeContainer>,
    pub success: &'static [Entry],
    pub failure: &'static [Entry],
}

impl Profile {
    /// Comando ainda sem tabela: toda chave fica fora dela.
    pub const EMPTY: Profile = Profile {
        label: "",
        nodes: None,
        success: &[],
        failure: &[],
    };

    pub fn entries(&self, outcome: Outcome) -> &'static [Entry] {
        match outcome {
            Outcome::Success => self.success,
            Outcome::Failure => self.failure,
        }
    }

    /// A entrada que declara a forma da chave, ou `None` quando a chave está fora da tabela.
    pub fn find(
        &self,
        outcome: Outcome,
        region: Region,
        node_type: Option<&str>,
        parent: Option<&str>,
        key: &str,
    ) -> Option<&'static Entry> {
        self.entries(outcome)
            .iter()
            .filter(|entry| entry.matches(region, node_type, key))
            .filter_map(|entry| entry.rank(parent).map(|rank| (rank, entry)))
            .min_by_key(|(rank, _)| *rank)
            .map(|(_, entry)| entry)
    }
}

/// Pontos de extensão do Template Method: cada comando de base escolhe o `Profile` e os trechos do `Doc`;
/// o que não sobrescreve vem da base (e do erro genérico).
pub trait Specialization {
    fn profile(&self) -> &'static Profile;

    fn state(&self, _value: &Value) -> State {
        State::Ok
    }

    /// As partes do cabeçalho depois do rótulo do comando.
    fn summary(&self, value: &Value) -> Vec<String>;

    fn detail_lines(&self, _value: &Value) -> Vec<Line> {
        Vec::new()
    }

    fn extras(&self, _value: &Value) -> Vec<Line> {
        Vec::new()
    }

    fn closing(&self, _value: &Value) -> Closing {
        Closing::None
    }

    /// `error` é o objeto sob `{"error": ...}`.
    fn failure_header(&self, error: &Value) -> Line {
        error::header(error)
    }

    fn failure_detail_lines(&self, detail: &Value, facts: &ErrorFacts) -> Vec<Line> {
        error::detail_lines(detail, facts)
    }

    fn failure_extras(&self, error: &Value, facts: &ErrorFacts) -> Vec<Line> {
        error::context_lines(&error["context"], 1, &facts.code, facts)
    }

    fn failure_closing(&self, facts: &ErrorFacts) -> Closing {
        error::closing(facts)
    }
}

pub struct BaseRenderer<S: Specialization>(pub S);

impl<S: Specialization> Renderer for BaseRenderer<S> {
    fn success(&self, value: &Value) -> Doc {
        let specialization = &self.0;
        let mut parts: Vec<String> = Some(specialization.profile().label)
            .filter(|label| !label.is_empty())
            .map(str::to_owned)
            .into_iter()
            .collect();
        parts.extend(specialization.summary(value));

        let mut lines = vec![Line::Header {
            state: specialization.state(value),
            parts,
        }];
        lines.extend(specialization.detail_lines(value));
        lines.extend(specialization.extras(value));

        Doc {
            lines,
            closing: specialization.closing(value),
        }
    }

    fn failure(&self, value: &Value) -> Doc {
        error::failure(&self.0, value)
    }
}

#[cfg(test)]
mod tests;
