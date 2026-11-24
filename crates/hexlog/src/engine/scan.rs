use std::borrow::Cow;

use serde_json::Value;

use crate::engine::model::{BodyLineage, Lineage, invalid_body, read_body};
use crate::engine::ports::{StoredRow, VerifyReads};
use crate::engine::verify::for_each_stored_row;
use crate::error::HexlogError;
use crate::ids::{Name, RelationTypeName, SemanticId, SessionLabel, TenantId};

/// Um record como foi gravado, sem a resolução de origem nem de status que as views fazem.
#[derive(Debug, Clone, PartialEq)]
pub struct ScannedRecord {
    pub seq: u64,
    pub at: String,
    pub lineage: Lineage,
    /// Tipo do node ou relation type, por isso `Name`.
    pub type_name: Name,
    pub retired: bool,
    pub data: Option<Value>,
    pub session: Option<SessionLabel>,
}

pub(crate) fn scan_records(
    reads: &mut dyn VerifyReads,
    tenant: &TenantId,
    visit: &mut dyn FnMut(ScannedRecord),
) -> Result<(), HexlogError> {
    for_each_stored_row(reads, tenant, |row| {
        visit(scanned(&row)?);
        Ok(())
    })
}

fn scanned(row: &StoredRow) -> Result<ScannedRecord, HexlogError> {
    let body = read_body(&row.body)?;
    let lineage = match &body.lineage {
        BodyLineage::Node { id } => {
            Lineage::Node(SemanticId::parse(id).map_err(|_| invalid_body())?)
        }
        BodyLineage::Relation {
            relation_type,
            from,
            to,
            ..
        } => Lineage::Relation {
            relation_type: RelationTypeName::parse(relation_type).map_err(|_| invalid_body())?,
            from: SemanticId::parse(from).map_err(|_| invalid_body())?,
            to: SemanticId::parse(to).map_err(|_| invalid_body())?,
        },
    };
    Ok(ScannedRecord {
        seq: u64::try_from(row.seq).map_err(|_| invalid_body())?,
        at: body.at.into_owned(),
        lineage,
        type_name: Name::parse(&body.type_name).map_err(|_| invalid_body())?,
        retired: body.retirement,
        data: body.data.map(Cow::into_owned),
        session: body.session,
    })
}
