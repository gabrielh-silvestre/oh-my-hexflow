use super::cursor::Cursor;

/// O que fazer com a próxima palavra lida, depois de um operador de redirecionamento.
#[derive(Default)]
pub(super) enum NextWord {
    /// Palavra comum do comando.
    #[default]
    Argument,
    /// Alvo do redirecionamento ou palavra do `<<<`: lida e descartada, nunca vira o programa.
    Ignored,
    /// Terminador de heredoc (`<<WORD`, `<<-WORD`).
    Delimiter { strip_tabs: bool },
}

/// Consome o resto do operador (`>>`, `>&`, `>|`, `<&`, `<<`, `<<-`, `<<<`) e diz o que a palavra seguinte é.
/// O `&` de `2>&1` e de `&>` pertence ao operador: nunca chega como separador.
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
