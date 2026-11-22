use hexlog::{ListFilter, Scalar};

use super::{WHERE_EQ_PAIRS_MAX, WHERE_EQ_TEXT_MAX};
use crate::error::{OmhlError, RuleCode};
use crate::gate::{GateQuestion, Question};

pub(crate) fn check_question_filters(questions: &[GateQuestion]) -> Result<(), OmhlError> {
    for (index, entry) in questions.iter().enumerate() {
        if let Some(filter) = question_filter(&entry.question) {
            check_where_eq(filter, &format!("/questions/{index}/filter"))?;
        }
    }

    Ok(())
}

fn question_filter(question: &Question) -> Option<&ListFilter> {
    match question {
        Question::Approved { of } => Some(of),
        Question::Occurred { select, .. } => Some(select),
        Question::NoPending { pending, .. } => Some(pending),
        Question::NoOpenContradiction { of } => of.as_ref(),
        Question::Anchored { select, .. } => Some(select),
    }
}

pub(crate) fn check_where_eq(filter: &ListFilter, at: &str) -> Result<(), OmhlError> {
    if filter.where_eq.len() > WHERE_EQ_PAIRS_MAX {
        return Err(OmhlError::rule(
            RuleCode::WhereEqTooMany,
            &format!("{at}/where_eq"),
        ));
    }
    let too_long = filter.where_eq.iter().position(
        |(_, value)| matches!(value, Scalar::Str(text) if text.chars().count() > WHERE_EQ_TEXT_MAX),
    );

    match too_long {
        Some(pair) => Err(OmhlError::rule(
            RuleCode::WhereEqValueTooLong,
            &format!("{at}/where_eq/{pair}"),
        )),
        None => Ok(()),
    }
}
