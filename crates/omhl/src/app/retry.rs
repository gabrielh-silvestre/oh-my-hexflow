use std::thread;
use std::time::Duration;

use hexlog::ErrorCode;

use super::error::AppError;

const BUSY_TIMEOUT: Duration = Duration::from_secs(2);
const PAUSES: [Duration; 2] = [Duration::from_millis(250), Duration::from_millis(500)];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    pub busy_timeout: Duration,
    pub pauses: Vec<Duration>,
}

impl Default for RetryPolicy {
    fn default() -> RetryPolicy {
        RetryPolicy {
            busy_timeout: BUSY_TIMEOUT,
            pauses: PAUSES.to_vec(),
        }
    }
}

impl RetryPolicy {
    /// Política da gravação depois do `check run`: o `busy_timeout` aqui não tem efeito, porque a
    /// conexão já abriu.
    pub fn after_run() -> RetryPolicy {
        RetryPolicy {
            busy_timeout: BUSY_TIMEOUT,
            pauses: [1, 2, 4, 8, 15, 30].map(Duration::from_secs).to_vec(),
        }
    }
}

/// Repete só o `BUSY`: nesse caso nada foi gravado.
pub(crate) fn with_retry<T>(
    policy: &RetryPolicy,
    mut attempt: impl FnMut() -> Result<T, AppError>,
) -> Result<T, AppError> {
    for pause in &policy.pauses {
        match attempt() {
            Err(error) if error.code() == ErrorCode::Busy.as_str() => thread::sleep(*pause),
            done => return done,
        }
    }

    attempt()
}
