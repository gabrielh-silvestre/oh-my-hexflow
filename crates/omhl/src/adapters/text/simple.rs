//! Texto dos comandos simples do ciclo: `project`, `check sweep`, `check retire`, `guideline retire`,
//! `hooks install`, `spec init` e `byproduct`. Todos usam a base e o erro genérico.

use serde_json::Value;

use super::base::{Entry, Profile, Region, Shape, Specialization};
use super::check_run::SECRET_WARNING;
use super::doc::{Closing, Line, counted, dash, field, items, joined, one_line, opt_text, text};
use super::error;

pub static PROJECT_PROFILE: Profile = Profile {
    label: "project",
    nodes: None,
    success: &[
        Entry::new(Region::Root, "dir", Shape::Closed),
        Entry::new(Region::Root, "written", Shape::Closed).optional(),
        // Os nomes que não mudaram não ajudam a decidir nada: o texto só os conta.
        Entry::new(Region::Root, "unchanged", Shape::Noise).optional(),
        Entry::new(Region::Root, "removed", Shape::Closed).optional(),
    ],
    failure: &error::ENTRIES,
};

pub static CHECK_SWEEP_PROFILE: Profile = Profile {
    label: "check sweep",
    nodes: None,
    success: &[
        Entry::new(Region::Root, "scan_timed_out", Shape::Derived),
        Entry::new(Region::Root, "unreadable", Shape::Derived),
        Entry::new(Region::Data, "comm", Shape::Closed)
            .under("swept")
            .optional(),
        Entry::new(Region::Data, "pid", Shape::Closed)
            .under("swept")
            .optional(),
        Entry::new(Region::Data, "owner_pid", Shape::Closed)
            .under("swept")
            .optional(),
    ],
    failure: &error::ENTRIES,
};

pub static CHECK_RETIRE_PROFILE: Profile = Profile {
    label: "check retire",
    nodes: None,
    success: &[Entry::new(Region::Root, "retired", Shape::Literal)],
    failure: &error::ENTRIES,
};

pub static GUIDELINE_RETIRE_PROFILE: Profile = Profile {
    label: "guideline retire",
    nodes: None,
    success: &[
        Entry::new(Region::Root, "retired", Shape::Literal),
        Entry::new(Region::Root, "superseded_by", Shape::Literal).optional(),
    ],
    failure: &error::ENTRIES,
};

pub static HOOKS_INSTALL_PROFILE: Profile = Profile {
    label: "hooks install",
    nodes: None,
    success: &[
        Entry::new(Region::Root, "hooks_path", Shape::Closed),
        Entry::new(Region::Root, "omhl", Shape::Closed),
        Entry::new(Region::Root, "scripts", Shape::Closed),
        Entry::new(Region::Data, "code", Shape::Closed)
            .under("warnings")
            .optional(),
        Entry::new(Region::Data, "path", Shape::Closed)
            .under("warnings")
            .optional(),
        Entry::new(Region::Data, "hint", Shape::Closed)
            .under("warnings")
            .optional(),
    ],
    failure: &error::ENTRIES,
};

pub static SPEC_INIT_PROFILE: Profile = Profile {
    label: "spec init",
    nodes: None,
    success: &[
        Entry::new(Region::Root, "work_item", Shape::Literal),
        Entry::new(Region::Root, "slug", Shape::Literal),
        Entry::new(Region::Root, "dir", Shape::Closed),
        Entry::new(Region::Root, "path", Shape::Closed),
    ],
    failure: &error::ENTRIES,
};

pub static BYPRODUCT_PROFILE: Profile = Profile {
    label: "byproduct",
    nodes: None,
    success: &[
        Entry::new(Region::Root, "check", Shape::Literal),
        Entry::new(Region::Root, "work_item", Shape::Literal),
        Entry::new(Region::Root, "outcome", Shape::Closed),
        // O hash só endereça a saída guardada; quem lê o texto quer as linhas.
        Entry::new(Region::Root, "byproduct", Shape::Noise),
        Entry::new(Region::Root, "lines", Shape::Literal).optional(),
        Entry::new(Region::Root, "truncated_lines", Shape::Derived).optional(),
    ],
    failure: &error::ENTRIES,
};

pub struct Project;

impl Specialization for Project {
    fn profile(&self) -> &'static Profile {
        &PROJECT_PROFILE
    }

    fn summary(&self, value: &Value) -> Vec<String> {
        vec![
            text(value, "dir"),
            format!("{} written", items(value, "written").count()),
            format!("{} unchanged", items(value, "unchanged").count()),
            format!("{} removed", items(value, "removed").count()),
        ]
    }

    fn detail_lines(&self, value: &Value) -> Vec<Line> {
        ["written", "removed"]
            .into_iter()
            .filter(|key| items(value, key).next().is_some())
            .map(|key| dash(vec![key.to_owned(), joined(value, key)]))
            .collect()
    }
}

pub struct CheckSweep;

fn scan_timed_out(value: &Value) -> bool {
    value["scan_timed_out"] == true
}

impl Specialization for CheckSweep {
    fn profile(&self) -> &'static Profile {
        &CHECK_SWEEP_PROFILE
    }

    fn symbol(&self, value: &Value) -> &'static str {
        match scan_timed_out(value) {
            true => "⚠",
            false => "✔",
        }
    }

    /// Formato fixo (`N swept`, `pid N`, `owner N`): o runner de exemplos normaliza os números.
    fn summary(&self, value: &Value) -> Vec<String> {
        let mut parts = vec![
            format!("{} swept", items(value, "swept").count()),
            format!("{} unreadable", value["unreadable"]),
        ];
        if scan_timed_out(value) {
            parts.push("scan timed out".to_owned());
        }

        parts
    }

    fn detail_lines(&self, value: &Value) -> Vec<Line> {
        items(value, "swept")
            .map(|process| {
                dash(vec![
                    text(process, "comm"),
                    format!("pid {}", process["pid"]),
                    format!("owner {}", process["owner_pid"]),
                ])
            })
            .collect()
    }

    fn closing(&self, value: &Value) -> Closing {
        match scan_timed_out(value) {
            true => Closing::Next(
                "run omhl check sweep again, an empty swept proves nothing after a timeout"
                    .to_owned(),
            ),
            false => Closing::None,
        }
    }
}

pub struct CheckRetire;

impl Specialization for CheckRetire {
    fn profile(&self) -> &'static Profile {
        &CHECK_RETIRE_PROFILE
    }

    fn summary(&self, value: &Value) -> Vec<String> {
        vec![format!("retired {}", text(value, "retired"))]
    }
}

pub struct GuidelineRetire;

impl Specialization for GuidelineRetire {
    fn profile(&self) -> &'static Profile {
        &GUIDELINE_RETIRE_PROFILE
    }

    fn summary(&self, value: &Value) -> Vec<String> {
        let mut parts = vec![format!("retired {}", text(value, "retired"))];
        if let Some(successor) = opt_text(value, "superseded_by") {
            parts.push(format!("superseded by {successor}"));
        }

        parts
    }
}

pub struct HooksInstall;

impl Specialization for HooksInstall {
    fn profile(&self) -> &'static Profile {
        &HOOKS_INSTALL_PROFILE
    }

    fn symbol(&self, value: &Value) -> &'static str {
        match items(value, "warnings").next() {
            Some(_) => "⚠",
            None => "✔",
        }
    }

    fn summary(&self, value: &Value) -> Vec<String> {
        let mut parts = vec![counted(items(value, "scripts").count(), "script")];
        let warnings = items(value, "warnings").count();
        if warnings > 0 {
            parts.push(counted(warnings, "warning"));
        }

        parts
    }

    fn detail_lines(&self, value: &Value) -> Vec<Line> {
        let paths = [
            field("hooks path", text(value, "hooks_path")),
            field("omhl", text(value, "omhl")),
            field("scripts", joined(value, "scripts")),
        ];
        let warnings = items(value, "warnings").map(|warning| {
            dash(vec![
                "warning".to_owned(),
                text(warning, "code"),
                text(warning, "path"),
            ])
        });

        paths.into_iter().chain(warnings).collect()
    }

    /// A dica do aviso é a ação: vai para o fecho, como a dica do erro.
    fn closing(&self, value: &Value) -> Closing {
        match items(value, "warnings").find_map(|warning| opt_text(warning, "hint")) {
            Some(hint) => Closing::Next(hint.trim_end_matches('.').to_owned()),
            None => Closing::None,
        }
    }
}

pub struct SpecInit;

impl Specialization for SpecInit {
    fn profile(&self) -> &'static Profile {
        &SPEC_INIT_PROFILE
    }

    fn summary(&self, value: &Value) -> Vec<String> {
        vec![text(value, "work_item"), text(value, "slug")]
    }

    fn detail_lines(&self, value: &Value) -> Vec<Line> {
        vec![
            field("dir", text(value, "dir")),
            field("path", text(value, "path")),
        ]
    }

    fn closing(&self, value: &Value) -> Closing {
        Closing::Next(format!(
            "fill in the five sections of the part file, then run omhl spec validate {}",
            text(value, "work_item")
        ))
    }
}

pub struct Byproduct;

impl Specialization for Byproduct {
    fn profile(&self) -> &'static Profile {
        &BYPRODUCT_PROFILE
    }

    fn summary(&self, value: &Value) -> Vec<String> {
        vec![
            text(value, "check"),
            text(value, "work_item"),
            text(value, "outcome"),
            counted(items(value, "lines").count(), "line"),
        ]
    }

    fn detail_lines(&self, value: &Value) -> Vec<Line> {
        items(value, "lines")
            .filter_map(Value::as_str)
            .map(|line| Line::Raw {
                depth: 1,
                text: one_line(line),
            })
            .collect()
    }

    /// O aviso de segredos vale para toda saída guardada de check (CA-10), nos dois layouts.
    fn extras(&self, value: &Value) -> Vec<Line> {
        let truncated = value["truncated_lines"].as_u64().map(|cut| Line::Note {
            symbol: "⚠",
            text: format!("{} cut short", counted(cut as usize, "line")),
        });

        truncated
            .into_iter()
            .chain([Line::Raw {
                depth: 0,
                text: SECRET_WARNING.to_owned(),
            }])
            .collect()
    }
}
