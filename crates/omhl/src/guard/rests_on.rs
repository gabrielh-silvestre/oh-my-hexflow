use crate::error::RuleCode;

const GUIDELINE: &str = "guideline";
const ARCHITECTURE_KIND: &str = "arquitetura";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RestsOnRefusal {
    TargetType,
    Architecture,
}

impl RestsOnRefusal {
    pub(super) fn code(self) -> RuleCode {
        match self {
            RestsOnRefusal::TargetType => RuleCode::RestsOnTargetType,
            RestsOnRefusal::Architecture => RuleCode::RestsOnArchitecture,
        }
    }
}

/// Lista branca: só uma diretriz tem limite, e ela só se apoia em diretriz de negócio.
/// Destino `None` (nativo ou inexistente) fica para o motor decidir.
pub(super) fn rests_on_allowed(
    source_type: &str,
    target: Option<(&str, Option<&str>)>,
) -> Result<(), RestsOnRefusal> {
    if source_type != GUIDELINE {
        return Ok(());
    }
    let Some((target_type, target_kind)) = target else {
        return Ok(());
    };
    if target_type != GUIDELINE {
        return Err(RestsOnRefusal::TargetType);
    }
    if target_kind == Some(ARCHITECTURE_KIND) {
        return Err(RestsOnRefusal::Architecture);
    }

    Ok(())
}
