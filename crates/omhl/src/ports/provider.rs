pub mod gh;

use std::time::Duration;

use crate::provider::Provider;
use crate::runner::CommandRunner;
use crate::workflow::config::{ProviderConfig, ProviderName};

pub use self::gh::{GH_PROGRAM, GhProvider};

pub fn build<'r>(
    config: &ProviderConfig,
    runner: &'r dyn CommandRunner,
    timeout: Duration,
) -> Box<dyn Provider + 'r> {
    match config.name {
        ProviderName::Github => Box::new(GhProvider::new(runner, &config.host, timeout)),
    }
}
