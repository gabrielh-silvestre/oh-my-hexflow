use hexlog::{ListFilter, Scalar};

use super::{WHERE_EQ_PAIRS_MAX, WHERE_EQ_TEXT_MAX};
use crate::error::{OmhlError, RuleCode};

/// Confere os tetos de `where_eq` antes de a consulta chegar ao `hexlog`.
pub(crate) fn check_where_eq(filter: &ListFilter) -> Result<(), OmhlError> {
    if filter.where_eq.len() > WHERE_EQ_PAIRS_MAX {
        return Err(OmhlError::rule(
            RuleCode::WhereEqTooMany,
            "/filter/where_eq",
        ));
    }
    let too_long = filter.where_eq.iter().position(
        |(_, value)| matches!(value, Scalar::Str(text) if text.chars().count() > WHERE_EQ_TEXT_MAX),
    );

    match too_long {
        Some(pair) => Err(OmhlError::rule(
            RuleCode::WhereEqValueTooLong,
            &format!("/filter/where_eq/{pair}"),
        )),
        None => Ok(()),
    }
}
