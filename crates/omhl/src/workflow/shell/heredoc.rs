use super::ReadError;
use super::cursor::Cursor;

/// Terminador de um heredoc ainda sem corpo lido; `<<-` ignora tabs no começo da linha do terminador.
pub(super) struct Delimiter {
    pub(super) tag: String,
    pub(super) strip_tabs: bool,
}

/// O corpo é dado, não comando: consome as linhas de cada heredoc pendente, na ordem, até a linha do terminador.
pub(super) fn skip_bodies(
    cursor: &mut Cursor,
    pending: &mut Vec<Delimiter>,
) -> Result<(), ReadError> {
    for delimiter in pending.drain(..) {
        loop {
            let line = cursor.read_line().ok_or(ReadError::UnclosedHeredoc)?;
            let line = if delimiter.strip_tabs {
                line.trim_start_matches('\t')
            } else {
                &line
            };
            if line == delimiter.tag {
                break;
            }
        }
    }

    Ok(())
}
