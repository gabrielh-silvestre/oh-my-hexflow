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
    // `with_snippet: false` e os caracteres de controle em espaço: a razão do hook sai numa linha só.
    let options = serde_saphyr::options! { no_schema: true, with_snippet: false };
    let parsed: Frontmatter =
        serde_saphyr::from_str_with_options(yaml, options).map_err(|error| {
            error
                .to_string()
                .replace(|character: char| character.is_control(), " ")
        })?;
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
mod tests;
