use crate::error::RuleCode;

const GUIDELINE: &str = "guideline";
const ARCHITECTURE_KIND: &str = "arquitetura";

/// Lista branca: só uma diretriz tem limite, e ela só se apoia em diretriz de negócio.
/// Destino `None` (nativo ou inexistente) fica para o motor decidir.
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
