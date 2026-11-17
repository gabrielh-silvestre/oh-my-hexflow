/// Prova um varredor contra trechos que devem e que não devem ser acusados, antes de varrer o código real.
pub fn prove_both_sides(
    count: impl Fn(&str) -> Result<usize, String>,
    flagged: &[&str],
    accepted: &[&str],
) -> Vec<String> {
    let mut failures = Vec::new();
    for snippet in flagged {
        match count(snippet) {
            Ok(0) => failures.push(format!("embedded snippet was not flagged: {snippet}")),
            Ok(_) => {}
            Err(error) => {
                failures.push(format!("embedded snippet did not lex ({error}): {snippet}"))
            }
        }
    }
    for snippet in accepted {
        match count(snippet) {
            Ok(0) => {}
            Ok(_) => failures.push(format!("embedded snippet was flagged: {snippet}")),
            Err(error) => {
                failures.push(format!("embedded snippet did not lex ({error}): {snippet}"))
            }
        }
    }

    failures
}
