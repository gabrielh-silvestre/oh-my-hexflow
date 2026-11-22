use crate::error::RuleCode;

const GUIDELINE: &str = "guideline";
const ARCHITECTURE_KIND: &str = "architecture";

pub(super) fn rests_on_allowed(
    source_type: &str,
    target: Option<(&str, Option<&str>)>,
) -> Result<(), RuleCode> {
    if source_type != GUIDELINE {
        return Ok(());
    }
    let Some((target_type, target_kind)) = target else {
        return Ok(());
    };
    if target_type != GUIDELINE {
        return Err(RuleCode::RestsOnTargetType);
    }
    if target_kind == Some(ARCHITECTURE_KIND) {
        return Err(RuleCode::RestsOnArchitecture);
    }

    Ok(())
}
