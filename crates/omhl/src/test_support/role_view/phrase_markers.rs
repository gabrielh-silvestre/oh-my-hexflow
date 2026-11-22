use super::finding;

pub struct Marker {
    pub rule: &'static str,
    pub file: &'static str,
    pub fragments: &'static [&'static str],
}

const INTERVIEW: &str = "skills/interview/SKILL.md";
const PLAN_CONSENSUS: &str = "skills/plan-consensus/SKILL.md";
const REVIEW: &str = "skills/review/SKILL.md";

pub const MARKERS: [Marker; 10] = [
    Marker {
        rule: "AC-1",
        file: INTERVIEW,
        fragments: &[
            "**Premissa da rodada.**",
            "confirme ou corrija na mesma rodada",
            "nada vai ao banco antes",
        ],
    },
    Marker {
        rule: "AC-1",
        file: INTERVIEW,
        fragments: &[
            "**Prova da confirmação.**",
            "`confirmed_by`",
            "2000 caracteres",
        ],
    },
    Marker {
        rule: "AC-3",
        file: INTERVIEW,
        fragments: &[
            "**Parada.**",
            "20% ou menos",
            "cada parte ativa",
            "premissa de negócio confirmada",
        ],
    },
    Marker {
        rule: "AC-3",
        file: INTERVIEW,
        fragments: &["**Cobertura.**", "parte ativa", "premissa confirmada"],
    },
    Marker {
        rule: "AC-5",
        file: PLAN_CONSENSUS,
        fragments: &[
            "`oh-my-hexflow:analyst`",
            "tardia (`origin: late`, `assumed`)",
        ],
    },
    Marker {
        rule: "AC-5",
        file: "agents/analyst.md",
        fragments: &["is late: record it with `origin` `late`", "`assumed`"],
    },
    Marker {
        rule: "AC-5",
        file: "agents/planner.md",
        fragments: &[
            "is late (`origin: late`)",
            "Never cite in the spec a late premise that is still `assumed`",
        ],
    },
    Marker {
        rule: "AC-5",
        file: "agents/critic.md",
        fragments: &["is late (`origin: late`)", "list it in the Open Questions"],
    },
    Marker {
        rule: "AC-8",
        file: REVIEW,
        fragments: &[
            "Premissa confirmada sem evidência",
            "`FAIL`",
            "`kind` `missing`",
        ],
    },
    Marker {
        rule: "AC-8",
        file: "agents/verifier.md",
        fragments: &[
            "A premise without evidence is a FAIL",
            "`FAIL`",
            "`kind` `missing`",
        ],
    },
];

pub fn phrase_markers(markers: &[Marker], files: &[(&str, &str)]) -> Vec<String> {
    let missing = markers.iter().filter_map(|marker| {
        let text = files
            .iter()
            .find(|(path, _)| *path == marker.file)
            .map_or("", |(_, text)| *text);
        let present = text.lines().any(|line| {
            marker
                .fragments
                .iter()
                .all(|fragment| line.contains(fragment))
        });

        (!present).then(|| {
            format!(
                "{} em {}: {}",
                marker.rule, marker.file, marker.fragments[0]
            )
        })
    });

    finding("frases marcadoras ausentes", missing)
        .into_iter()
        .collect()
}
