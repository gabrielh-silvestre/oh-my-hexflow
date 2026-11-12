//! `App` sobre um banco temporário com uma raiz de projeto temporária, para os testes das ações
//! que leem e escrevem `.omhl/`. Cada chamada abre o `App` de novo: o banco está em disco, então o
//! estado vale entre as chamadas, e o `AppFixture` não precisa guardar um empréstimo de si mesmo.

use ::omhl::app::App;
use ::omhl::app::action::{Action, Caller};
use ::omhl::app::error::AppError;
use ::omhl::app::retry::RetryPolicy;
use ::omhl::facade::OmhlDeps;
use ::omhl::probe::{ProbeOutcome, TreeSource};
use hexlog::{Snapshot, SnapshotKind};
use serde_json::Value;

use super::ext::ExtTree;
use super::omhl::acme;
use super::raw::RawHarness;
use super::{FakeProbe, snap};

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

    fn app(&self) -> App<'_> {
        let deps = OmhlDeps {
            clock: &self.raw.clock,
            ids: &self.raw.ids,
            probe: &self.probe,
        };
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
