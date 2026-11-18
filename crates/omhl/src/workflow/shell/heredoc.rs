use super::ReadError;
use super::cursor::Cursor;

/// Terminador de um heredoc ainda sem corpo lido; `<<-` ignora tabs no começo da linha do terminador.
pub(super) struct Delimiter {
    pub(super) tag: String,
    pub(super) strip_tabs: bool,
    /// Terminador com aspas ou barra: o bash não expande o corpo.
    pub(super) quoted: bool,
}

/// O corpo é dado, não comando: consome as linhas de cada heredoc pendente, na ordem, até a linha do terminador.
/// Devolve se algum corpo sem aspas tem `$(` ou crase, que o bash executa.
pub(super) fn skip_bodies(
    cursor: &mut Cursor,
    pending: &mut Vec<Delimiter>,
) -> Result<bool, ReadError> {
    let mut executes = false;
    for delimiter in pending.drain(..) {
        loop {
            let line = cursor.read_line().ok_or(ReadError::UnclosedHeredoc)?;
            let closing = if delimiter.strip_tabs {
                line.trim_start_matches('\t')
            } else {
                &line
            };
            if closing == delimiter.tag {
                break;
            }
            executes |= !delimiter.quoted && (line.contains("$(") || line.contains('`'));
        }
    }

    Ok(executes)
}
