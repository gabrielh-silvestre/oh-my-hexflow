pub mod gh;

use std::time::Duration;

use crate::provider::Provider;
use crate::runner::CommandRunner;
use crate::workflow::config::{ProviderConfig, ProviderName};

pub use self::gh::{GH_PROGRAM, GhProvider};

/// Limite de cada chamada do `gh` no `omhl merge`.
pub const MERGE_PROVIDER_TIMEOUT: Duration = Duration::from_secs(30);

pub fn build<'r>(
    config: &ProviderConfig,
    runner: &'r dyn CommandRunner,
    timeout: Duration,
) -> Box<dyn Provider + 'r> {
    match config.name {
        ProviderName::Github => Box::new(GhProvider::new(runner, &config.host, timeout)),
    }
}

/// Os leitores da branch padrão que rodam sem pedido tiram também os tokens Enterprise fora do `github.com`.
pub fn build_unattended<'r>(
    config: &ProviderConfig,
    runner: &'r dyn CommandRunner,
    timeout: Duration,
) -> Box<dyn Provider + 'r> {
    match config.name {
        ProviderName::Github => {
            Box::new(GhProvider::new(runner, &config.host, timeout).unattended())
        }
    }
}
