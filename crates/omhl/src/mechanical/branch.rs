use hexlog::SemanticId;

/// Fatos crus do HEAD, montados pela sonda; `head` é `None` em repositório sem commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadFacts {
    pub current: Option<String>,
    pub rebase_head: Option<String>,
    pub containing: Vec<String>,
    pub head: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchError {
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchKind {
    Main,
    Develop,
    WorkItem(SemanticId),
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushPlan {
    NothingToAttest,
    CheckHead,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushError {
    ProtectedTarget {
        remote_ref: String,
    },
    NotCheckedOut {
        local_ref: String,
        remote_ref: String,
    },
    Malformed {
        line: usize,
    },
}

pub fn current_branch(_facts: &HeadFacts) -> Result<String, BranchError> {
    unimplemented!()
}

pub fn classify(_branch: &str) -> BranchKind {
    unimplemented!()
}

pub fn plan_push(
    _lines: &str,
    _head: Option<&str>,
    _checkout: Option<&str>,
) -> Result<PushPlan, PushError> {
    unimplemented!()
}
