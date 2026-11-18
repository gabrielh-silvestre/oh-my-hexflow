//! Esqueleto dos comandos de base: o `BaseRenderer` monta o `Doc` por Template Method e o `Profile` declara a forma de cada chave do `Value`.

use serde_json::Value;

use super::Renderer;
use super::doc::{Closing, Doc, Line};
use super::error::{self, ErrorFacts};

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
    /// Rótulo do comando no cabeçalho.
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

    fn symbol(&self, _value: &Value) -> &'static str {
        "✔"
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
        let mut parts = vec![specialization.profile().label.to_owned()];
        parts.extend(specialization.summary(value));

        let mut lines = vec![Line::Header {
            symbol: specialization.symbol(value),
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
mod tests {
    use super::*;

    fn profile(success: &'static [Entry], failure: &'static [Entry]) -> Profile {
        Profile {
            label: "test",
            nodes: None,
            success,
            failure,
        }
    }

    fn shape_of(
        profile: &Profile,
        node_type: Option<&str>,
        parent: Option<&str>,
        key: &str,
    ) -> Option<Shape> {
        profile
            .find(Outcome::Success, Region::Data, node_type, parent, key)
            .map(|entry| entry.shape)
    }

    #[test]
    fn it_should_prefer_the_explicit_entry_with_the_same_parent_over_one_without_parent() {
        const ENTRIES: &[Entry] = &[
            Entry::new(Region::Data, "id", Shape::Literal),
            Entry::new(Region::Data, "id", Shape::Hash(8)).under("snapshot"),
        ];
        let table = profile(ENTRIES, &[]);

        let shape = shape_of(&table, None, Some("snapshot"), "id");

        assert_eq!(shape, Some(Shape::Hash(8)));
    }

    #[test]
    fn it_should_prefer_an_explicit_entry_without_parent_over_a_node_type_wildcard() {
        const ENTRIES: &[Entry] = &[
            Entry::new(Region::Data, WILDCARD, Shape::Noise).scoped("check"),
            Entry::new(Region::Data, "how", Shape::Literal),
        ];
        let table = profile(ENTRIES, &[]);

        let shape = shape_of(&table, Some("check"), None, "how");

        assert_eq!(shape, Some(Shape::Literal));
    }

    #[test]
    fn it_should_prefer_the_node_type_wildcard_over_the_any_wildcard() {
        const ENTRIES: &[Entry] = &[
            Entry::new(Region::Data, WILDCARD, Shape::Noise),
            Entry::new(Region::Data, WILDCARD, Shape::Closed).scoped("check"),
        ];
        let table = profile(ENTRIES, &[]);

        let shape = shape_of(&table, Some("check"), None, "anything");

        assert_eq!(shape, Some(Shape::Closed));
    }

    #[test]
    fn it_should_fall_back_to_the_any_wildcard_when_no_other_entry_matches() {
        const ENTRIES: &[Entry] = &[
            Entry::new(Region::Data, WILDCARD, Shape::Noise),
            Entry::new(Region::Data, WILDCARD, Shape::Closed).scoped("check"),
        ];
        let table = profile(ENTRIES, &[]);

        let shape = shape_of(&table, Some("premise"), None, "anything");

        assert_eq!(shape, Some(Shape::Noise));
    }

    #[test]
    fn it_should_prefer_the_node_type_entry_over_the_any_entry_for_the_same_key() {
        const ENTRIES: &[Entry] = &[
            Entry::new(Region::Root, "id", Shape::Literal),
            Entry::new(Region::Root, "id", Shape::Closed).scoped("attestation"),
        ];
        let table = profile(ENTRIES, &[]);

        let shape = table
            .find(
                Outcome::Success,
                Region::Root,
                Some("attestation"),
                None,
                "id",
            )
            .map(|entry| entry.shape);

        assert_eq!(shape, Some(Shape::Closed));
    }

    #[test]
    fn it_should_use_the_any_entry_for_a_node_type_without_its_own_entry() {
        const ENTRIES: &[Entry] = &[
            Entry::new(Region::Root, "id", Shape::Literal),
            Entry::new(Region::Root, "id", Shape::Closed).scoped("attestation"),
        ];
        let table = profile(ENTRIES, &[]);

        let shape = table
            .find(Outcome::Success, Region::Root, Some("decision"), None, "id")
            .map(|entry| entry.shape);

        assert_eq!(shape, Some(Shape::Literal));
    }

    #[test]
    fn it_should_skip_an_explicit_entry_whose_parent_differs_from_the_asked_one() {
        const ENTRIES: &[Entry] =
            &[Entry::new(Region::Data, "id", Shape::Hash(8)).under("snapshot")];
        let table = profile(ENTRIES, &[]);

        let shape = shape_of(&table, None, Some("gate"), "id");

        assert_eq!(shape, None);
    }

    #[test]
    fn it_should_not_let_the_wildcard_match_outside_the_data_region() {
        const ENTRIES: &[Entry] = &[Entry::new(Region::Root, WILDCARD, Shape::Noise)];
        let table = profile(ENTRIES, &[]);

        let found = table.find(Outcome::Success, Region::Root, None, None, "anything");

        assert_eq!(found, None);
    }

    #[test]
    fn it_should_return_none_for_a_key_outside_the_table() {
        const ENTRIES: &[Entry] = &[Entry::new(Region::Root, "created", Shape::Literal)];
        let table = profile(ENTRIES, &[]);

        let found = table.find(Outcome::Success, Region::Root, None, None, "made");

        assert_eq!(found, None);
    }

    #[test]
    fn it_should_declare_only_the_leaves_of_an_object_never_the_object_key() {
        const ENTRIES: &[Entry] =
            &[Entry::new(Region::Data, "id", Shape::Hash(8)).under("snapshot")];
        let table = profile(ENTRIES, &[]);

        let object = table.find(Outcome::Success, Region::Data, None, None, "snapshot");

        assert_eq!(object, None);
    }

    #[test]
    fn it_should_search_only_the_table_of_the_asked_outcome() {
        const SUCCESS: &[Entry] = &[Entry::new(Region::Root, "ok", Shape::Derived)];
        const FAILURE: &[Entry] = &[Entry::new(Region::Root, "code", Shape::Closed)];
        let table = profile(SUCCESS, FAILURE);

        let found = table.find(Outcome::Failure, Region::Root, None, None, "ok");

        assert_eq!(found, None);
    }
}
