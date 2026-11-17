/// Os sete papéis que o `OMHL_ROLE` aceita; lista fechada, o valor é gravado como `agent` do record.
const ROLES: [&str; 7] = [
    "omhl:analyst",
    "omhl:planner",
    "omhl:architect",
    "omhl:critic",
    "omhl:code-reviewer",
    "omhl:security-reviewer",
    "omhl:verifier",
];

/// Devolve o papel estático igual ao valor, ou `None` se estiver fora da lista.
pub(crate) fn known_role(value: &str) -> Option<&'static str> {
    ROLES.into_iter().find(|role| *role == value)
}
