#!/bin/sh
# Wraps the plugin hooks: without omhl on the PATH it warns instead of blocking the work.
# The branch without omhl uses only shell builtins, because the hook PATH may have nothing.

hook=$1

# Text already escaped for JSON (quotes as \" and line breaks as \n), ready to go inside a string.
INSTALL_CMD='[ \"$(uname -sm)\" = \"Linux x86_64\" ] && mkdir -p ~/.local/bin && (cd \"$(mktemp -d)\" && curl -fsSLO https://github.com/gabrielh-silvestre/oh-my-hexflow/releases/latest/download/omhl-x86_64-unknown-linux-musl.tar.gz && curl -fsSLO https://github.com/gabrielh-silvestre/oh-my-hexflow/releases/latest/download/omhl-x86_64-unknown-linux-musl.tar.gz.sha256 && sha256sum -c omhl-x86_64-unknown-linux-musl.tar.gz.sha256 && tar -xzf omhl-x86_64-unknown-linux-musl.tar.gz -C ~/.local/bin omhl)'
INSTALL_HELP='The prebuilt binary exists only for Linux x86_64; the command above downloads nothing elsewhere. Make sure ~/.local/bin is on the PATH of the Claude Code process, then restart the session. On other platforms build from source with `cargo build --release`.'

# Reads X.Y.Z (pre-release suffix ignored) into $major, $minor and $patch; accepts only characters that fit in JSON without escaping.
parse_version() {
  case $1 in '' | *[!0-9A-Za-z.+-]*) return 1 ;; esac
  core=${1%%[-+]*}
  case $core in '' | *[!0-9.]*) return 1 ;; esac
  IFS=.
  set -- $core
  unset IFS
  [ $# -eq 3 ] || return 1
  for part in "$@"; do
    # A leading zero would turn octal in shell arithmetic.
    case $part in '' | 0?*) return 1 ;; esac
  done
  major=$1 minor=$2 patch=$3
}

# $1.$2.$3 is older than $4.$5.$6, comparing major, minor and patch in that order.
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

# Any read that fails leaves the warning out: without a readable version there is nothing to compare.
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

# Always exits 0: an omhl older than lang1 lacks the subcommand and exits 2, and Claude Code would show a hook error on every subagent.
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

# Climbs from the directory looking for the marker, without looking inside .omhl/ folders.
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

# Accepts only an absolute "cwd" with no backslash; stdin is the fallback for when CLAUDE_PROJECT_DIR is missing.
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
