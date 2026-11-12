use std::sync::Barrier;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use ::omhl::runner::{CommandRunner, RunError, RunReport, RunSpec};
use hexlog::{AppendRequest, Clock, Config, Deps, Hexlog, IdGenerator, TenantId};
use uuid::Uuid;

use super::omhl::{node_item, premise_data};
use super::runner::FakeRunner;
use super::{EmptyNative, FixedClock, SeqIds, author};

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

/// Relógio cujo `now()` espera o teste largar o `Sender` que ele entrega em `attempts`. O `Hexlog`
/// lê o relógio uma vez por `append`, logo antes de pedir a trava de escrita: cada leitura marca uma
/// tentativa de gravação. Depois que o teste larga o receptor, as leituras não esperam.
pub struct AttemptClock {
    inner: FixedClock,
    attempts: Sender<Sender<()>>,
}

impl AttemptClock {
    pub fn new() -> (AttemptClock, Receiver<Sender<()>>) {
        let (attempts, reached) = mpsc::channel();
        let clock = AttemptClock {
            inner: FixedClock::default(),
            attempts,
        };

        (clock, reached)
    }
}

impl Clock for AttemptClock {
    fn now(&self) -> String {
        let (resume, resumed) = mpsc::channel::<()>();
        if self.attempts.send(resume).is_ok()
            && let Err(RecvTimeoutError::Timeout) = resumed.recv_timeout(DEADLINE * 6)
        {
            panic!("the append attempt was never resumed");
        }

        self.inner.now()
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
    hold_write_lock(config, tenant, |release, _| body(release))
}

/// Como `with_write_lock`, mas a soltura vem da tentativa `attempt` de `append`, contada pelo
/// `AttemptClock` que deu `attempts`: as anteriores esgotam o `busy_timeout` e dão `Busy`; a de número
/// `attempt` só segue depois que o node do segurador foi gravado, então pega a trava livre sem
/// depender de quanto o disco demora.
pub fn with_write_lock_until_attempt<R>(
    config: &Config,
    tenant: &TenantId,
    attempts: Receiver<Sender<()>>,
    attempt: usize,
    body: impl FnOnce() -> R,
) -> R {
    hold_write_lock(config, tenant, |release, written| {
        thread::scope(|scope| {
            scope.spawn(move || {
                // largar o `Sender` recebido deixa a tentativa seguir; estourar o prazo só solta a
                // trava, e o assert do teste é quem reprova
                let last = (1..attempt)
                    .try_for_each(|_| attempts.recv_timeout(DEADLINE).map(drop))
                    .and_then(|()| attempts.recv_timeout(DEADLINE));
                release.now();
                let _ = written.recv_timeout(DEADLINE * 6);
                drop(last);
            });

            body()
        })
    })
}

/// `body` recebe a soltura e o aviso de que o node do segurador foi gravado (a trava está livre).
fn hold_write_lock<R>(
    config: &Config,
    tenant: &TenantId,
    body: impl FnOnce(&Release, Receiver<()>) -> R,
) -> R {
    let (locked, lock_taken) = mpsc::channel();
    let (committed, written) = mpsc::channel();

    thread::scope(|scope| {
        scope.spawn(move || {
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
            // ninguém esperando o aviso: nada a fazer
            let _ = committed.send(());
        });

        let release = Release(
            lock_taken
                .recv_timeout(DEADLINE)
                .expect("the write lock was not taken within 5 seconds"),
        );
        drop(lock_taken);

        body(&release, written)
    })
}

/// Runner que segura a PRIMEIRA chamada do argv `gate` até a trava do banco estar tomada (passa
/// por `reached` e depois por `locked`); as outras chamadas vão direto ao `inner`. Torna
/// determinístico "o 1º append grava, o 2º fica Busy".
pub struct GatedRunner<'g> {
    pub inner: FakeRunner,
    pub gate: Vec<String>,
    pub reached: &'g Barrier,
    pub locked: &'g Barrier,
    pub passed: &'g AtomicBool,
}

impl CommandRunner for GatedRunner<'_> {
    fn run(&self, spec: &RunSpec) -> Result<RunReport, RunError> {
        if spec.argv == self.gate && !self.passed.swap(true, Ordering::SeqCst) {
            self.reached.wait();
            self.locked.wait();
        }

        self.inner.run(spec)
    }
}

/// Numa thread do `thread::scope` do teste: espera o `GatedRunner` chegar ao gate, toma a trava,
/// libera o runner por `locked` e segura a trava até `done` (ou 30 s).
pub fn lock_at_gate(
    config: &Config,
    tenant: &TenantId,
    reached: &Barrier,
    locked: &Barrier,
    done: Receiver<()>,
) {
    reached.wait();
    with_write_lock(config, tenant, move |_| {
        locked.wait();
        // estourar o prazo só solta a trava; o assert do teste é quem reprova
        let _ = done.recv_timeout(Duration::from_secs(30));
    });
}

/// Runner que, dentro de `run()`, abre outra conexão e grava `premises.concurrent`; manda o
/// resultado do append em `appended` e depois responde como o `inner`.
pub struct AppendingRunner<'a> {
    pub inner: FakeRunner,
    pub config: Config,
    pub tenant: &'a TenantId,
    pub ids: &'a SeqIds,
    pub appended: Sender<Result<(), String>>,
}

impl CommandRunner for AppendingRunner<'_> {
    fn run(&self, spec: &RunSpec) -> Result<RunReport, RunError> {
        let clock = FixedClock::default();
        let deps = Deps {
            clock: &clock,
            ids: self.ids,
            catalog: ::omhl::graphs::catalog().expect("the embedded catalogs build"),
            native: &EmptyNative,
        };
        let request = AppendRequest {
            tenant: self.tenant.clone(),
            author: author(),
            key: None,
            items: vec![node_item(
                "premises.concurrent",
                "premise",
                premise_data("written while a command runs", None),
            )],
        };
        let appended = Hexlog::open(&self.config, deps)
            .and_then(|mut hexlog| hexlog.append(&request))
            .map(|_| ())
            .map_err(|error| error.to_string());
        // o teste pode já ter largado o Receiver; a resposta do runner não muda
        let _ = self.appended.send(appended);

        self.inner.run(spec)
    }
}
