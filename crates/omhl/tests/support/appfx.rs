//! `App` sobre um banco temporário com uma raiz de projeto temporária, para os testes das ações
//! que leem e escrevem `.omhl/`. Cada chamada abre o `App` de novo: o banco está em disco, então o
//! estado vale entre as chamadas, e o `AppFixture` não precisa guardar um empréstimo de si mesmo.

use std::fs;

use ::omhl::app::App;
use ::omhl::app::action::{Action, Caller};
use ::omhl::app::error::AppError;
use ::omhl::app::retry::RetryPolicy;
use ::omhl::facade::{Omhl, OmhlDeps};
use ::omhl::probe::{ProbeOutcome, TreeSource};
use hexlog::{RelationQuery, Snapshot, SnapshotKind};
use serde_json::{Value, json};

use super::ext::ExtTree;
use super::omhl::acme;
use super::raw::RawHarness;
use super::{FakeProbe, rty, snap};

/// Prefixo dos slugs de `guideline_retire_refusals`: um teste que o ache numa recusa viu o slug ecoado.
pub const RETIRE_SLUG_MARK: &str = "zq-";

/// A árvore do HEAD que a sonda limpa responde ao fechamento.
fn head_tree() -> Snapshot {
    Snapshot::new(SnapshotKind::GitTree, &"c".repeat(40)).expect("valid snapshot")
}

pub struct AppFixture {
    raw: RawHarness,
    probe: FakeProbe,
    tree: ExtTree,
    with_root: bool,
}

impl AppFixture {
    /// Tenant `acme`, banco novo e raiz de projeto = diretório temporário vazio.
    pub fn new() -> AppFixture {
        AppFixture {
            raw: RawHarness::new(),
            probe: FakeProbe::clean(snap('a')).with_tree(
                TreeSource::Head,
                Ok(ProbeOutcome::Reproducible(head_tree())),
            ),
            tree: ExtTree::new(),
            with_root: true,
        }
    }

    /// `App` sem raiz de projeto: `load` e `project` respondem `no-project-root`.
    pub fn without_root() -> AppFixture {
        AppFixture {
            with_root: false,
            ..AppFixture::new()
        }
    }

    pub fn ext(&self) -> &ExtTree {
        &self.tree
    }

    /// Relatório do `load` em `Ok`; o JSON `{"error": ...}` em `Err`.
    pub fn load(&self, dry_run: bool) -> Result<Value, Value> {
        self.run(&Caller::cli(), &Action::Load { dry_run })
    }

    pub fn project(&self) -> Result<Value, Value> {
        self.run(&Caller::cli(), &Action::Project)
    }

    /// `append` com o autor `cli`/`user`; `request` é o corpo do pedido (`items`, `key`, ...).
    pub fn append_as_user(&self, request: Value) -> Result<Value, Value> {
        self.run(&Caller::cli(), &Action::Append { request })
    }

    /// `append` com o autor do comando `guideline retire`, o único client que a guarda deixa retirar uma diretiva.
    pub fn append_as_guideline_retirer(&self, request: Value) -> Result<Value, Value> {
        self.run(
            &Caller::guideline_retirer("user"),
            &Action::Append { request },
        )
    }

    /// Escreve o arquivo `.omhl/guidelines/<slug>.toml` de uma diretiva `business`.
    pub fn write_guideline_file(&self, slug: &str) {
        let toml = format!("kind = \"business\"\ntext = \"Be brief ({slug})\"\n");
        self.tree.guideline(slug, &toml);
    }

    pub fn remove_guideline_file(&self, slug: &str) {
        let path = self
            .tree
            .root()
            .join(format!(".omhl/guidelines/{slug}.toml"));
        fs::remove_file(path).expect("the guideline file is removed");
    }

    /// Carrega uma diretiva por slug e apaga os arquivos: os nodes seguem vigentes, órfãos.
    pub fn load_orphaned_guidelines(&self, slugs: &[&str]) {
        for slug in slugs {
            self.write_guideline_file(slug);
        }
        self.load(false).expect("the guidelines load");
        for slug in slugs {
            self.remove_guideline_file(slug);
        }
    }

    /// `append` de uma relation `supersedes` (`from` é a nova, `to` a antiga) com o autor `cli`/`user`.
    pub fn append_supersedes(&self, from: &str, to: &str) -> Result<Value, Value> {
        self.append_as_user(json!({
            "items": [{
                "body": {
                    "kind": "relation",
                    "relation_type": "supersedes",
                    "from": from,
                    "to": to,
                }
            }]
        }))
    }

    /// Provoca as cinco recusas do comando `guideline retire`, nesta ordem: `guideline-not-found`,
    /// `guideline-not-orphaned`, `successor-not-current`, `successor-is-self` e `supersedes-cycle`.
    /// Devolve o JSON de cada erro; os slugs levam `RETIRE_SLUG_MARK`.
    pub fn guideline_retire_refusals(&self) -> Vec<Value> {
        let [old, new, mid, live] = ["zq-old", "zq-new", "zq-mid", "zq-live"];
        for slug in [old, new, mid, live] {
            self.write_guideline_file(slug);
        }
        self.load(false).expect("the guidelines load");
        for slug in [old, new, mid] {
            self.remove_guideline_file(slug);
        }
        let id = |slug: &str| format!("guidelines.{slug}");
        let not_found = self.guideline_retire(&id("zq-ghost"), None);
        let not_orphaned = self.guideline_retire(&id(live), None);
        let successor_missing = self.guideline_retire(&id(old), Some(&id("zq-ghost")));
        let successor_self = self.guideline_retire(&id(old), Some(&id(old)));
        self.append_supersedes(&id(old), &id(mid))
            .expect("old supersedes mid");
        self.append_supersedes(&id(mid), &id(new))
            .expect("mid supersedes new");
        let cycle = self.guideline_retire(&id(old), Some(&id(new)));

        [
            not_found,
            not_orphaned,
            successor_missing,
            successor_self,
            cycle,
        ]
        .into_iter()
        .map(|result| result.expect_err("the retirement is refused"))
        .collect()
    }

    /// `omhl guideline retire <id> [--by <by>]` com o autor `cli`/`user`.
    pub fn guideline_retire(&self, id: &str, by: Option<&str>) -> Result<Value, Value> {
        let action = Action::GuidelineRetire {
            id: id.to_owned(),
            by: by.map(str::to_owned),
        };

        self.run(&Caller::cli(), &action)
    }

    /// A resposta de `render`: o texto do contexto da sessão e a contagem de omitidos.
    pub fn render(&self) -> Value {
        self.run(&Caller::cli(), &Action::Render)
            .unwrap_or_else(|error| panic!("render is accepted: {error}"))
    }

    /// O relatório do gate de PR do work item; um gate que reprova é um relatório, não um erro.
    pub fn gate_pr(&self, work_item: &str) -> Value {
        let action = Action::GatePr {
            work_item: work_item.to_owned(),
        };

        self.run(&Caller::cli(), &action)
            .unwrap_or_else(|error| panic!("the gate is evaluated: {error}"))
    }

    /// As relations do tipo, inclusive as de extremo aposentado, como pares `(from, to)`.
    pub fn relation_edges(&self, relation_type: &str) -> Vec<(String, String)> {
        let query = RelationQuery {
            relation_type: Some(rty(relation_type)),
            from: None,
            to: None,
            include_retired: true,
        };
        let relations = self
            .omhl()
            .relations(&acme(), &query)
            .unwrap_or_else(|error| panic!("relations are read: {error}"));

        relations
            .into_iter()
            .map(|relation| {
                (
                    relation.from.as_str().to_owned(),
                    relation.to.as_str().to_owned(),
                )
            })
            .collect()
    }

    /// O `Omhl` real sobre o mesmo banco, para ler o que a `App` não expõe (revisões, relations).
    pub fn omhl(&self) -> Omhl<'_> {
        Omhl::open(&self.raw.config(), self.deps())
            .unwrap_or_else(|error| panic!("omhl opens: {error}"))
    }

    pub fn verify(&self) -> Value {
        self.run(&Caller::cli(), &Action::Verify)
            .unwrap_or_else(|error| panic!("verify is accepted: {error}"))
    }

    /// Todos os nodes vigentes do tipo (todas as páginas), como um array JSON.
    pub fn list(&self, type_name: &str) -> Value {
        let mut items: Vec<Value> = Vec::new();
        let mut after_seq = None;

        loop {
            let action = Action::List {
                type_name: Some(type_name.to_owned()),
                id_prefix: None,
                limit: None,
                after_seq,
                where_eq: Vec::new(),
            };
            let page = self
                .run(&Caller::cli(), &action)
                .unwrap_or_else(|error| panic!("list is accepted: {error}"));
            let page_items = page["items"].as_array().expect("the page has items");
            items.extend(page_items.iter().cloned());
            after_seq = page["next_after_seq"].as_u64();
            if after_seq.is_none() {
                break;
            }
        }

        Value::Array(items)
    }

    /// O objeto `node` da resposta de `get`.
    pub fn get(&self, id: &str) -> Value {
        let action = Action::Get { id: id.to_owned() };
        let mut response = self
            .run(&Caller::cli(), &action)
            .unwrap_or_else(|error| panic!("get is accepted: {error}"));

        response["node"].take()
    }

    fn run(&self, caller: &Caller, action: &Action) -> Result<Value, Value> {
        self.app()
            .dispatch(caller, action)
            .map_err(|error| error.to_json())
    }

    fn deps(&self) -> OmhlDeps<'_> {
        OmhlDeps {
            clock: &self.raw.clock,
            ids: &self.raw.ids,
            probe: &self.probe,
            session: &crate::support::NO_SESSION,
        }
    }

    fn app(&self) -> App<'_> {
        let deps = self.deps();
        let opened: Result<App<'_>, AppError> =
            App::open(&self.raw.db_path(), &acme(), deps, &RetryPolicy::default());
        let app = opened.unwrap_or_else(|error| panic!("the app opens: {error}"));

        if self.with_root {
            app.with_project_root(self.tree.root())
        } else {
            app
        }
    }
}

impl Default for AppFixture {
    fn default() -> AppFixture {
        AppFixture::new()
    }
}
