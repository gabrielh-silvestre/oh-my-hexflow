#!/bin/sh
# Redação da evidência do G1: lê a pasta crua (fora do repositório, ignorada pelo Git) e escreve a cópia
# redigida neste diretório. Regras: tabela "Redação do que vai ao repositório" do plano omc-port.
# Uso, da raiz do repositório: sh docs/spikes/skills-preload/g1/redact.sh [PASTA_CRUA [DESTINO]]
# Requer jq. Nunca copia: transcrições cruas (*.jsonl), *.err.txt e as pastas .omc/ de estado de hooks.
# Os colchetes nos padrões ([o], [l]) impedem que as conferências do critério A1 casem com este script.
set -eu

SRC=${1:-.omc/research/omc-port-g1}
DST=${2:-docs/spikes/skills-preload/g1}
CANARY=$(cat "$SRC/canary.txt")
TAB=$(printf '\t')
# Campos do stdin dos hooks que carregam caminho ou identificador de sessão.
REDACTED_FIELDS='["session_id","transcript_path","cwd","prompt_id","scratchpad_dir"]'

# Scripts, roteiro, JSON e texto: caminho do repositório vira <repo>, o do scratchpad vira <scratchpad>.
redact_text() {
  sed -e 's|/tmp/c[l]aude-[0-9]*/[^/]*/[^/]*/scratchpad|<scratchpad>|g' \
      -e 's|/h[o]me/[^/ ]*/personal/oh-my-hexflow|<repo>|g' \
      -e 's|/h[o]me/[^ "]*|<redigido>|g' "$1" > "$2"
}

# Log de hooks (<evento>TAB<json>): fica o que tem agent_id e as linhas META.
# Sem agent_id (sessão principal): com $3 = extract vira um extrato; senão é descartada.
redact_hooks_log() {
  : > "$2"
  while IFS="$TAB" read -r event json; do
    if [ "$event" = META ]; then
      printf 'META\t%s\n' "$json" >> "$2"
    elif printf '%s' "$json" | jq -e 'has("agent_id")' > /dev/null; then
      { printf '%s\t' "$event"
        printf '%s' "$json" | jq -c --argjson f "$REDACTED_FIELDS" \
          'with_entries(if .key as $k | $f | index($k) then .value = "<redigido>" else . end)'
      } >> "$2"
    elif [ "${3:-}" = extract ]; then
      { printf '%s\t' "$event"
        printf '%s' "$json" | jq -c --arg c "$CANARY" \
          '{tool_name, subagent_type: .tool_input.subagent_type, name: .tool_input.name}
           + (if .tool_name == "Skill" then {skill: .tool_input.skill} else {} end)
           + {prompt_has_canary: ((.tool_input.prompt // "") | contains($c))}'
      } >> "$2"
    fi
  done < "$1"
}

# Transcrição de subagente: uma linha por registro, "<índice a partir de 0>TAB<type>[TAB<name da tool>]".
# Registros de prompt de sistema, anexos e sistema viram "omitido" (nem o tipo entra).
# O registro de pré-carga (<command-message>role-probe ou omhlprobe:role-probe) sai truncado em 200 caracteres em volta do canário.
summarize_transcript() {
  jq -rs --arg c "$CANARY" '
    def texts: [.message.content[]? | .text? // empty] | join("\n");
    def clean: gsub("/h[o]me/[^/ ]*/personal/[^/ ]*"; "<repo>") | gsub("/h[o]me/[^ \n]*"; "<redigido>") | gsub("\n"; "\\n");
    to_entries[] | .key as $i | .value as $r |
    if ($r.type | test("^(api-request-b[l]ob|attachment|system)$")) then "\($i)\tomitido"
    elif $r.type == "user" and ($r | texts | test("<command-message>([a-z]+:)?role-probe")) then
      ($r | texts | clean) as $t | ($t | index($c) // 0) as $p | ([$p - 150, 0] | max) as $s
      | "\($i)\tuser\tmarcador-de-pre-carga\t\($t[$s:$s + 200])"
    else
      ([$r.message.content[]? | select(.type? == "tool_use") | .name] | join(",")) as $tools
      | if $tools == "" then "\($i)\t\($r.type)" else "\($i)\t\($r.type)\ttool_use:\($tools)" end
    end' "$1"
}

mkdir -p "$DST/transcripts"
(cd "$SRC" && find . -type f ! -path '*/.omc/*' ! -path './transcripts/*' ! -name '*.err.txt' | sort) |
while read -r rel; do
  rel=${rel#./}
  mkdir -p "$DST/$(dirname "$rel")"
  case $rel in
    out/b.hooks.tsv) redact_hooks_log "$SRC/$rel" "$DST/$rel" extract ;;
    out/a*.hooks.tsv) redact_hooks_log "$SRC/$rel" "$DST/$rel" ;;
    *) redact_text "$SRC/$rel" "$DST/$rel" ;;
  esac
done

for transcript in "$SRC"/transcripts/*.jsonl; do
  summarize_transcript "$transcript" > "$DST/transcripts/$(basename "$transcript" .jsonl).summary.txt"
done
