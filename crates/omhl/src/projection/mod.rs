pub mod build;
pub mod write;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectedFile {
    pub name: String,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectionError {
    pub path: String,
    pub code: &'static str,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WriteReport {
    pub written: Vec<String>,
    pub unchanged: Vec<String>,
    pub removed: Vec<String>,
}

pub struct ProjectionState {
    pub nodes: Vec<hexlog::NodeView>,
    pub relations: Vec<hexlog::RelationView>,
}
