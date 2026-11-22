//! `NodeView` e `RelationView` montados à mão, sem banco: os campos que o teste não nomeia
//! recebem valores fixos e determinísticos.

use hexlog::{
    Author, EndpointStatus, NodeView, Origin, RecordId, RelationView, Snapshot, SnapshotKind,
};
use serde_json::Value;

use super::{DEFAULT_INSTANT, rty, sid, tname};

const RECORD_ID_PREFIX: &str = "test:0192a000-0000-7000-8000-";
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;
const RECORD_SUFFIX_MASK: u64 = 0xffff_ffff_ffff;

/// Record id válido e estável por semente: a mesma semente dá sempre o mesmo id.
fn derived_record_id(seed: &str) -> RecordId {
    let hash = seed.bytes().fold(FNV_OFFSET, |acc, byte| {
        (acc ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
    });

    RecordId::parse(&format!(
        "{RECORD_ID_PREFIX}{:012x}",
        hash & RECORD_SUFFIX_MASK
    ))
    .expect("valid record id")
}

fn view_author(client: &str) -> Author {
    Author {
        agent: "tester".to_owned(),
        model: None,
        client: client.to_owned(),
    }
}

pub fn node_view(id: &str, type_name: &str, data: Value) -> NodeView {
    NodeView {
        id: sid(id),
        type_name: tname(type_name),
        record_id: derived_record_id(id),
        seq: 1,
        at: DEFAULT_INSTANT.to_owned(),
        data,
        snapshot: None,
        external_id: None,
        author: view_author("tests"),
        shadowed: false,
    }
}

pub fn relation_view(name: &str, from: &str, to: &str) -> RelationView {
    RelationView {
        record_id: derived_record_id(&format!("{name}|{from}|{to}")),
        seq: 1,
        at: DEFAULT_INSTANT.to_owned(),
        relation_type: rty(name),
        from: sid(from),
        to: sid(to),
        from_origin: Origin::Tenant,
        to_origin: Origin::Tenant,
        from_status: EndpointStatus::Current,
        to_status: EndpointStatus::Current,
        native_catalog: None,
        external_id: None,
        author: view_author("tests"),
        retired: false,
    }
}

/// `NodeView` não guarda "aposentado": um node aposentado não é um `NodeView`, e o que o teste
/// vê dele é o extremo de relation com status `Retired` (`RelationViewExt::retired_from`).
pub trait NodeViewExt {
    fn shadowed(self) -> NodeView;
    fn author(self, client: &str) -> NodeView;
    /// `record_id` inteiro, no formato `<tenant>:<uuid v7>`.
    fn record(self, id: &str) -> NodeView;
    fn snapshot(self, kind: SnapshotKind, id: &str) -> NodeView;
    fn seq(self, seq: u64) -> NodeView;
    fn at(self, instant: &str) -> NodeView;
}

impl NodeViewExt for NodeView {
    fn shadowed(mut self) -> NodeView {
        self.shadowed = true;
        self
    }

    fn author(mut self, client: &str) -> NodeView {
        self.author = view_author(client);
        self
    }

    fn record(mut self, id: &str) -> NodeView {
        self.record_id = RecordId::parse(id).expect("valid record id");
        self
    }

    fn snapshot(mut self, kind: SnapshotKind, id: &str) -> NodeView {
        self.snapshot = Some(Snapshot::new(kind, id).expect("valid snapshot"));
        self
    }

    fn seq(mut self, seq: u64) -> NodeView {
        self.seq = seq;
        self
    }

    fn at(mut self, instant: &str) -> NodeView {
        self.at = instant.to_owned();
        self
    }
}

pub trait RelationViewExt {
    fn retired(self) -> RelationView;
    fn author(self, client: &str) -> RelationView;
    fn retired_from(self) -> RelationView;
    fn retired_to(self) -> RelationView;
    fn seq(self, seq: u64) -> RelationView;
    fn at(self, instant: &str) -> RelationView;
}

impl RelationViewExt for RelationView {
    fn retired(mut self) -> RelationView {
        self.retired = true;
        self
    }

    fn author(mut self, client: &str) -> RelationView {
        self.author = view_author(client);
        self
    }

    fn retired_from(mut self) -> RelationView {
        self.from_status = EndpointStatus::Retired;
        self
    }

    fn retired_to(mut self) -> RelationView {
        self.to_status = EndpointStatus::Retired;
        self
    }

    fn seq(mut self, seq: u64) -> RelationView {
        self.seq = seq;
        self
    }

    fn at(mut self, instant: &str) -> RelationView {
        self.at = instant.to_owned();
        self
    }
}
