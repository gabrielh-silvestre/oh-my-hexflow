use hexlog::{Name, SemanticId};

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

const BRANCH_PREFIX: &str = "refs/heads/";
const MAIN_REF: &str = "refs/heads/main";

pub fn current_branch(facts: &HeadFacts) -> Result<String, BranchError> {
    if let Some(current) = facts.current.as_deref().filter(|name| !name.is_empty()) {
        return Ok(current.to_owned());
    }
    // `detached HEAD\n` (rebase iniciado em HEAD solto) não tem o prefixo e vale como ausente.
    if let Some(name) = facts
        .rebase_head
        .as_deref()
        .and_then(|head| head.strip_prefix(BRANCH_PREFIX))
    {
        return Ok(name.strip_suffix('\n').unwrap_or(name).to_owned());
    }
    match facts.containing.as_slice() {
        [only] => Ok(only.clone()),
        _ => Err(BranchError::Ambiguous),
    }
}

pub fn classify(branch: &str) -> BranchKind {
    match branch {
        "main" => BranchKind::Main,
        "develop" => BranchKind::Develop,
        _ => work_item_id(branch).map_or(BranchKind::Other, BranchKind::WorkItem),
    }
}

/// `<tipo>/wi-<id>__<slug>`: o `<id>` vai até o primeiro `__` e precisa ser nome de um segmento.
fn work_item_id(branch: &str) -> Option<SemanticId> {
    let (kind, rest) = branch.split_once('/')?;
    let (id, slug) = rest.strip_prefix("wi-")?.split_once("__")?;
    if kind.is_empty() || slug.is_empty() {
        return None;
    }
    Name::parse(id).ok()?;
    SemanticId::parse(&format!("work-items.{id}")).ok()
}

/// Lê todas as linhas da stdin do `pre-push` antes de decidir (DP-15).
pub fn plan_push(
    lines: &str,
    head: Option<&str>,
    checkout: Option<&str>,
) -> Result<PushPlan, PushError> {
    let parsed: Vec<Option<[&str; 4]>> = lines.lines().map(push_fields).collect();
    if parsed
        .iter()
        .flatten()
        .any(|[_, _, rref, _]| *rref == MAIN_REF)
    {
        return Err(PushError::ProtectedTarget {
            remote_ref: MAIN_REF.to_owned(),
        });
    }
    if let Some(index) = parsed.iter().position(Option::is_none) {
        return Err(PushError::Malformed { line: index + 1 });
    }
    let pushed: Vec<[&str; 4]> = parsed
        .into_iter()
        .flatten()
        .filter(|[_, lsha, rref, _]| {
            !lsha.bytes().all(|byte| byte == b'0') && !rref.starts_with("refs/tags/")
        })
        .collect();
    if let Some([lref, _, rref, _]) = pushed
        .iter()
        .find(|[lref, lsha, _, _]| !is_checked_out(lref, lsha, head, checkout))
    {
        return Err(PushError::NotCheckedOut {
            local_ref: (*lref).to_owned(),
            remote_ref: (*rref).to_owned(),
        });
    }
    Ok(if pushed.is_empty() {
        PushPlan::NothingToAttest
    } else {
        PushPlan::CheckHead
    })
}

/// `<lref> <lsha> <rref> <rsha>`, separados por um espaço, nenhum vazio.
fn push_fields(line: &str) -> Option<[&str; 4]> {
    let fields: [&str; 4] = line.split(' ').collect::<Vec<_>>().try_into().ok()?;
    fields
        .iter()
        .all(|field| !field.is_empty())
        .then_some(fields)
}

fn is_checked_out(lref: &str, lsha: &str, head: Option<&str>, checkout: Option<&str>) -> bool {
    let same_branch = match (lref.strip_prefix(BRANCH_PREFIX), checkout) {
        (Some(branch), Some(current)) => branch == current,
        _ => true,
    };
    head == Some(lsha) && same_branch
}
