#!/bin/sh
# Envolve os hooks do plugin: sem o omhl no PATH ele avisa em vez de travar o trabalho.
# O ramo sem omhl usa só builtins do shell, porque o PATH do hook pode não ter nada.

hook=$1

# Texto já escapado para JSON (aspas como \" e quebras como \n), pronto para ir dentro de uma string.
INSTALL_CMD='[ \"$(uname -sm)\" = \"Linux x86_64\" ] && mkdir -p ~/.local/bin && (cd \"$(mktemp -d)\" && curl -fsSLO https://github.com/gabrielh-silvestre/oh-my-hexflow/releases/latest/download/omhl-x86_64-unknown-linux-musl.tar.gz && curl -fsSLO https://github.com/gabrielh-silvestre/oh-my-hexflow/releases/latest/download/omhl-x86_64-unknown-linux-musl.tar.gz.sha256 && sha256sum -c omhl-x86_64-unknown-linux-musl.tar.gz.sha256 && tar -xzf omhl-x86_64-unknown-linux-musl.tar.gz -C ~/.local/bin omhl)'
INSTALL_HELP='The prebuilt binary exists only for Linux x86_64; the command above downloads nothing elsewhere. Make sure ~/.local/bin is on the PATH of the Claude Code process, then restart the session. On other platforms build from source with `cargo build --release`.'

# Lê X.Y.Z (sufixo de pré-lançamento ignorado) em $major, $minor e $patch; só aceita caracteres que cabem num JSON sem escape.
parse_version() {
  case $1 in '' | *[!0-9A-Za-z.+-]*) return 1 ;; esac
  core=${1%%[-+]*}
  case $core in '' | *[!0-9.]*) return 1 ;; esac
  IFS=.
  set -- $core
  unset IFS
  [ $# -eq 3 ] || return 1
  for part in "$@"; do
    # Zero à esquerda viraria octal na aritmética do shell.
    case $part in '' | 0?*) return 1 ;; esac
  done
  major=$1 minor=$2 patch=$3
}

# $1.$2.$3 é mais velha que $4.$5.$6, comparando major, minor e patch nessa ordem.
is_older() {
  [ "$1" -ne "$4" ] && {
    [ "$1" -lt "$4" ]
    return
  }
  [ "$2" -ne "$5" ] && {
    [ "$2" -lt "$5" ]
    return
  }
  [ "$3" -lt "$6" ]
}

# Qualquer leitura que falhe deixa o aviso de fora: sem versão legível não há o que comparar.
version_check() {
  case $0 in
    */*) min_file=${0%/*}/min-omhl-version ;;
    *) min_file=./min-omhl-version ;;
  esac
  [ -r "$min_file" ] || return 0
  IFS= read -r min < "$min_file" || [ -n "$min" ]
  parse_version "$min" || return 0
  min_major=$major min_minor=$minor min_patch=$patch
  found=$(omhl --version 2>/dev/null) || return 0
  case $found in
    "omhl "*) found=${found#omhl } ;;
    *) return 0 ;;
  esac
  parse_version "$found" || return 0
  is_older "$major" "$minor" "$patch" "$min_major" "$min_minor" "$min_patch" || return 0
  warning="oh-my-hexflow: the omhl binary is version $found, older than the minimum $min the plugin requires. Update it with:\\n$INSTALL_CMD\\n$INSTALL_HELP"
  printf '{"systemMessage":"%s","hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"%s"}}\n' "$warning" "$warning"
}

# Sempre sai com 0: um omhl anterior à lang1 não tem o subcomando, sai com 2, e o Claude Code mostraria um erro de hook a cada subagente.
subagent_start() {
  output=$(omhl hook subagent-start 2>/dev/null) || return 0
  [ -z "$output" ] || printf '%s\n' "$output"
}

if command -v omhl >/dev/null 2>&1; then
  case $hook in
    version-check) version_check ;;
    spec-lock) omhl hook spec-lock || exit 2 ;;
    agent-role) omhl hook agent-role || exit 1 ;;
    session-start) exec omhl hook session-start ;;
    subagent-start) subagent_start ;;
  esac
  exit 0
fi

# Sobe a partir do diretório procurando o marcador, sem olhar dentro de pastas .omhl/.
has_marker() {
  d=$1
  while :; do
    case $d in
      */.omhl | */.omhl/*) ;;
      *) [ -f "$d/.omhl.toml" ] && return 0 ;;
    esac
    case $d in
      */*) d=${d%/*} ;;
      *) return 1 ;;
    esac
  done
}

# Só aceita um "cwd" absoluto e sem barra invertida; o stdin é reserva para quando CLAUDE_PROJECT_DIR falta.
project_dir() {
  if [ -n "$CLAUDE_PROJECT_DIR" ]; then
    printf '%s' "$CLAUDE_PROJECT_DIR"
    return
  fi
  IFS= read -r line || [ -n "$line" ]
  case $line in
    *'"cwd"'*)
      rest=${line#*'"cwd"'}
      case $rest in
        :*)
          rest=${rest#:}
          while :; do
            case $rest in
              ' '*) rest=${rest# } ;;
              *) break ;;
            esac
          done
          case $rest in
            '"/'*)
              value=${rest#\"}
              value=${value%%\"*}
              case $value in
                *\\*) ;;
                *)
                  printf '%s' "$value"
                  return
                  ;;
              esac
              ;;
          esac
          ;;
      esac
      ;;
  esac
  printf '%s' "$PWD"
}

case $hook in
  spec-lock)
    if has_marker "$(project_dir)"; then
      reason="oh-my-hexflow: the omhl binary is not on the PATH and this project is marked with .omhl.toml, so file edits are blocked. Install it with:\\n$INSTALL_CMD\\n$INSTALL_HELP"
      printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"%s"}}\n' "$reason"
    fi
    ;;
  version-check)
    warning="oh-my-hexflow: the omhl binary is not on the PATH, so the plugin hooks are inactive. Install it with:\\n$INSTALL_CMD\\n$INSTALL_HELP"
    printf '{"systemMessage":"%s","hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"%s"}}\n' "$warning" "$warning"
    ;;
esac
exit 0
