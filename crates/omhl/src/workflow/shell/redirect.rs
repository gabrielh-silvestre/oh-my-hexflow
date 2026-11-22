use super::cursor::Cursor;

#[derive(Default)]
pub(super) enum NextWord {
    #[default]
    Argument,
    Ignored,
    /// Terminador de heredoc (`<<WORD`, `<<-WORD`).
    Delimiter {
        strip_tabs: bool,
    },
}

pub(super) fn after_operator(operator: char, cursor: &mut Cursor) -> NextWord {
    match operator {
        '<' if cursor.eat('<') => {
            if cursor.eat('<') {
                NextWord::Ignored
            } else {
                NextWord::Delimiter {
                    strip_tabs: cursor.eat('-'),
                }
            }
        }
        '<' => {
            cursor.eat('&');
            NextWord::Ignored
        }
        _ => {
            let _ = cursor.eat('>') || cursor.eat('&') || cursor.eat('|');
            NextWord::Ignored
        }
    }
}
