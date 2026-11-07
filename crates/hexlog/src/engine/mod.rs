// removido na T6
#![allow(dead_code)]

pub(crate) mod chain;
pub(in crate::engine) mod decide;
pub(crate) mod limits;
pub(crate) mod model;
pub(crate) mod ports;
pub(in crate::engine) mod rules;
pub(crate) mod snapshot;

pub(crate) use decide::{DecideCtx, decide};
// A Facade valida o instante do relógio ao criar o tenant.
pub(crate) use rules::instant_is_well_formed;
