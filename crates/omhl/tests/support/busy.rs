use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use hexlog::{AppendRequest, Config, Deps, Hexlog, IdGenerator, TenantId};
use uuid::Uuid;

use super::omhl::{node_item, premise_data};
use super::{EmptyNative, FixedClock, author};

const DEADLINE: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(1);
const UUID_V7_HOLDER: u128 = 0x0192_b000_0000_7000_8000_0000_0000_0000;

/// Solta a trava de escrita segura por `with_write_lock`.
pub struct Release<'r>(&'r AtomicBool);

impl Release<'_> {
    pub fn now(&self) {
        self.0.store(true, Ordering::Release);
    }
}

/// Solta a trava ao sair do escopo, inclusive quando `body` entra em pânico.
struct ReleaseOnDrop<'r>(&'r AtomicBool);

impl Drop for ReleaseOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

/// Gerador que espera dentro de `next()`: o motor o chama dentro da transação de escrita, então
/// a trava fica tomada até `release` mudar.
struct WaitingIds<'r> {
    locked: &'r AtomicBool,
    release: &'r AtomicBool,
    counter: AtomicU64,
}

impl IdGenerator for WaitingIds<'_> {
    fn next(&self) -> Uuid {
        self.locked.store(true, Ordering::Release);
        let started = Instant::now();
        while !self.release.load(Ordering::Acquire) && started.elapsed() < DEADLINE * 6 {
            thread::sleep(POLL);
        }
        let n = self.counter.fetch_add(1, Ordering::Relaxed);

        Uuid::from_u128(UUID_V7_HOLDER | u128::from(n))
    }
}

/// Segura a trava de escrita do banco enquanto `body` roda: grava um node pelo `Hexlog` com um
/// `IdGenerator` que espera dentro de `next()`. O tenant já deve existir. `Release::now()` solta a
/// trava; ao fim de `body` ela é solta.
pub fn with_write_lock<R>(
    config: &Config,
    tenant: &TenantId,
    body: impl FnOnce(&Release<'_>) -> R,
) -> R {
    let locked = AtomicBool::new(false);
    let released = AtomicBool::new(false);

    thread::scope(|scope| {
        scope.spawn(|| {
            let ids = WaitingIds {
                locked: &locked,
                release: &released,
                counter: AtomicU64::new(1),
            };
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

        let _guard = ReleaseOnDrop(&released);
        let started = Instant::now();
        while !locked.load(Ordering::Acquire) {
            assert!(
                started.elapsed() < DEADLINE,
                "the write lock was not taken within 5 seconds"
            );
            thread::sleep(POLL);
        }

        body(&Release(&released))
    })
}
