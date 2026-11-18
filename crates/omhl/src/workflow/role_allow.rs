//! O `omhl-allow` do frontmatter de um agente: as folhas do `omhl` que o papel pode rodar.

use serde::Deserialize;

use super::omhl_leaf::Leaf;

const FRONTMATTER_FENCE: &str = "---";
const APPEND_LEAF: &str = "append";

#[derive(Debug, Deserialize)]
struct Frontmatter {
    #[serde(rename = "omhl-allow")]
    omhl_allow: Option<RoleAllow>,
}

/// Os tipos do `append` só são validados aqui; quem os impõe é o binário (`guard`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppendAllow {
    pub nodes: Vec<String>,
    pub relations: Vec<String>,
    pub retire: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleAllow {
    pub commands: Vec<String>,
    pub append: Option<AppendAllow>,
}

impl RoleAllow {
    pub fn permits(&self, leaf: &Leaf) -> bool {
        match leaf {
            Leaf::ReadOnly => true,
            Leaf::Named(name) => self.commands.contains(name),
        }
    }
}

/// O arquivo inteiro é validado: um erro de forma em qualquer parte nega até um comando de leitura.
pub fn parse_role_allow(agent_file: &str) -> Result<RoleAllow, String> {
    let yaml = frontmatter(agent_file).ok_or("the agent file has no frontmatter")?;
    // `no_schema`: um item como `1` ou `true` não vira texto sozinho; o YAML precisa citá-lo.
    let options = serde_saphyr::options! { no_schema: true };
    let parsed: Frontmatter =
        serde_saphyr::from_str_with_options(yaml, options).map_err(|error| error.to_string())?;
    let allow = parsed
        .omhl_allow
        .ok_or("the frontmatter has no omhl-allow")?;
    if allow.commands.iter().any(|command| command == APPEND_LEAF) != allow.append.is_some() {
        return Err("omhl-allow has an append block exactly when commands lists append".to_owned());
    }

    Ok(allow)
}

/// O texto entre a primeira linha `---` e a seguinte.
fn frontmatter(agent_file: &str) -> Option<&str> {
    let after_fence = agent_file.strip_prefix(FRONTMATTER_FENCE)?;
    let body = after_fence
        .strip_prefix("\r\n")
        .or_else(|| after_fence.strip_prefix('\n'))?;
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        if line.trim_end() == FRONTMATTER_FENCE {
            return Some(&body[..offset]);
        }
        offset += line.len();
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_with(omhl_allow: &str) -> String {
        format!("---\nname: x\n{omhl_allow}---\nbody\n")
    }

    #[test]
    fn it_should_read_a_matrix_with_and_without_append() {
        let with = parse_role_allow(&file_with(
            "omhl-allow:\n  commands: [append, list]\n  append:\n    nodes: [decision]\n    relations: []\n    retire: []\n",
        ))
        .expect("valid matrix");
        let without = parse_role_allow(&file_with("omhl-allow:\n  commands: [list]\n"))
            .expect("valid matrix");

        assert_eq!(with.commands, ["append", "list"]);
        assert!(with.append.is_some());
        assert!(without.append.is_none());
    }

    #[test]
    fn it_should_refuse_what_has_the_wrong_shape() {
        let bad_matrices = [
            "omhl-allow:\n  commands: [list]\n  extra: 1\n",
            "omhl-allow:\n  commands: [append]\n",
            "omhl-allow:\n  commands: [list]\n  append:\n    nodes: []\n    relations: []\n    retire: []\n",
            "omhl-allow:\n  commands: [append]\n  append:\n    nodes: x\n    relations: []\n    retire: []\n",
            "omhl-allow:\n  commands: [append]\n  append:\n    nodes: [1]\n    relations: []\n    retire: []\n",
            "omhl-allow:\n  commands: [append]\n  append:\n    nodes: []\n    relations: []\n    retire: []\n    other: []\n",
            "omhl-allow:\n  commands: [list, 1]\n",
            "name: y\n",
        ];

        for matrix in bad_matrices {
            assert!(parse_role_allow(&file_with(matrix)).is_err(), "{matrix}");
        }
    }

    #[test]
    fn it_should_refuse_a_file_whose_matrix_is_outside_the_frontmatter() {
        let in_body = "---\nname: x\n---\nomhl-allow:\n  commands: [list]\n";

        assert!(parse_role_allow(in_body).is_err());
        assert!(parse_role_allow("---\nname: x\n").is_err());
        assert!(parse_role_allow("omhl-allow:\n  commands: [list]\n").is_err());
    }

    #[test]
    fn it_should_permit_read_only_leaves_to_every_role() {
        let allow =
            parse_role_allow(&file_with("omhl-allow:\n  commands: [list]\n")).expect("valid");

        assert!(allow.permits(&Leaf::ReadOnly));
        assert!(allow.permits(&Leaf::Named("list".to_owned())));
        assert!(!allow.permits(&Leaf::Named("attest".to_owned())));
    }
}
