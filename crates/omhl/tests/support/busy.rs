use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use hexlog::{AppendRequest, Config, Deps, Hexlog, IdGenerator, TenantId};
use uuid::Uuid;

use super::omhl::{node_item, premise_data};
use super::{EmptyNative, FixedClock, author};

const DEADLINE: Duration = Duration::from_secs(5);

/// Solta a trava de escrita segura por `with_write_lock`.
pub struct Release(Sender<()>);

impl Release {
    pub fn now(&self) {
        // A trava já pode ter sido solta (o receptor sumiu): nada a fazer.
        let _ = self.0.send(());
    }
}

/// Gerador que espera dentro de `next()`: o motor o chama dentro da transação de escrita, então
/// a trava fica tomada até o `Release` mandar o sinal ou ser largado. Cada chamada que espera
/// entrega ao teste o próprio canal de soltura; depois que o teste larga o receptor, as chamadas
/// seguintes não esperam.
struct WaitingIds {
    locked: Sender<Sender<()>>,
}

impl IdGenerator for WaitingIds {
    fn next(&self) -> Uuid {
        let (release, released) = mpsc::channel();
        if self.locked.send(release).is_ok() {
            match released.recv_timeout(DEADLINE * 6) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => {}
                Err(RecvTimeoutError::Timeout) => panic!("the write lock was never released"),
            }
        }

        Uuid::now_v7()
    }
}

/// Segura a trava de escrita do banco enquanto `body` roda: grava um node pelo `Hexlog` com um
/// `IdGenerator` que espera dentro de `next()`. O tenant já deve existir. `Release::now()` solta a
/// trava; ao fim de `body` (ou se ele entrar em pânico) ela é solta.
pub fn with_write_lock<R>(
    config: &Config,
    tenant: &TenantId,
    body: impl FnOnce(&Release) -> R,
) -> R {
    let (locked, lock_taken) = mpsc::channel();

    thread::scope(|scope| {
        scope.spawn(|| {
            let ids = WaitingIds { locked };
            let clock = FixedClock::default();
            let deps = Deps {
                clock: &clock,
                ids: &ids,
                catalog: ::omhl::graphs::catalog().expect("the embedded catalogs build"),
                native: &EmptyNative,
            };
            let mut hexlog =
                Hexlog::open(config, deps).expect("the lock holder opens the database");
            let request = AppendRequest {
                tenant: tenant.clone(),
                author: author(),
                key: None,
                items: vec![node_item(
                    "premises.lock-holder",
                    "premise",
                    premise_data("holds the write lock", None),
                )],
            };
            hexlog
                .append(&request)
                .expect("the lock holder writes its node");
        });

        let release = Release(
            lock_taken
                .recv_timeout(DEADLINE)
                .expect("the write lock was not taken within 5 seconds"),
        );
        drop(lock_taken);

        body(&release)
    })
}
