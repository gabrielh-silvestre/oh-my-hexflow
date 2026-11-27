use hexlog::Author;
use serde_json::Value;

use crate::error::{OmhlError, RuleCode};
use crate::guard::{GUIDELINE_RETIRE_CLIENT, known_role};

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    List {
        type_name: Option<String>,
        id_prefix: Option<String>,
        limit: Option<u32>,
        after_seq: Option<u64>,
        where_eq: Vec<(String, String)>,
    },
    Get {
        id: String,
    },
    Append {
        request: Value,
    },
    Attest {
        check: String,
        work_item: String,
        outcome: String,
        note: Option<String>,
    },
    Render,
    GatePr {
        work_item: String,
    },
    GateWorktree {
        branch: String,
    },
    Close {
        work_item: String,
        note: Option<String>,
    },
    Abandon {
        work_item: String,
        reason: String,
        note: String,
    },
    Verify,
    Load {
        dry_run: bool,
    },
    Project,
    CheckRun {
        target: CheckTarget,
        pre_push: Option<String>,
    },
    CheckRetire {
        check: String,
    },
    GuidelineRetire {
        id: String,
        by: Option<String>,
    },
    ConsumeDevelopMark,
    Byproduct {
        check: String,
        work_item: String,
        tail: u32,
    },
    ByproductPrune,
    CheckSweep,
    HooksInstall {
        force: bool,
    },
    SpecInit {
        work_item: String,
        slug: String,
    },
    SpecValidate {
        work_item: String,
    },
    ReportAnswers {
        work_item: Option<String>,
    },
    ReportRecurrences {
        min: Option<u32>,
    },
    ReportRetro {
        work_item: String,
    },
    ReportSessions {
        work_items: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckTarget {
    Event(String),
    Check(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caller {
    pub client: &'static str,
    pub agent: &'static str,
}

impl Caller {
    pub fn cli() -> Caller {
        Caller {
            client: "cli",
            agent: "user",
        }
    }

    /// O Claude Code define `CLAUDECODE=1` nos subprocessos que lança (Bash, hooks, tmux) e as extensões
    /// de IDE o definem nos terminais integrados; o falso positivo (humano) é o erro seguro, e o falso negativo
    /// (variável removida) é o Follow-up da trava técnica.
    pub fn cli_under_claude_code() -> Caller {
        Caller {
            client: "cli",
            agent: "claude",
        }
    }

    pub fn cli_with_role(claude_code: bool, role: Option<&str>) -> Result<Caller, OmhlError> {
        match (claude_code, role) {
            (true, None) => Ok(Caller::cli_under_claude_code()),
            (false, None) => Ok(Caller::cli()),
            (true, Some(role)) => known_role(role)
                .map(|agent| Caller {
                    agent,
                    ..Caller::cli()
                })
                .ok_or_else(|| OmhlError::rule(RuleCode::RoleUnknown, "")),
            (false, Some(_)) => Err(OmhlError::rule(RuleCode::RoleUnknown, "")),
        }
    }

    pub fn hook() -> Caller {
        Caller {
            client: "hook",
            agent: "hook",
        }
    }

    pub fn loader() -> Caller {
        Caller {
            client: "loader",
            agent: "loader",
        }
    }

    pub fn guideline_retirer(agent: &'static str) -> Caller {
        Caller {
            client: GUIDELINE_RETIRE_CLIENT,
            agent,
        }
    }

    pub fn executor() -> Caller {
        Caller {
            client: "executor",
            agent: "executor",
        }
    }

    /// Sem `model`: o `omhl` não sabe qual modelo está do outro lado.
    pub(crate) fn author(&self) -> Author {
        Author {
            agent: self.agent.to_owned(),
            model: None,
            client: self.client.to_owned(),
        }
    }
}
