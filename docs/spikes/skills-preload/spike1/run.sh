#!/bin/sh
# Sequential spike runs: 4 combinations (agent probe/probe-q x with/without name).
S=<sandbox>
mkdir -p "$S/work" "$S/out"
cd "$S/work" || exit 1
run() {
  tag="$1"; agent="$2"; name="$3"
  export SPIKE_LOG="$S/out/$tag.hooks.log"
  : > "$SPIKE_LOG"
  if [ -n "$name" ]; then
    extra=" and name \"$name\""
  else
    extra=" and NO name parameter"
  fi
  timeout 240 claude -p \
    --plugin-dir "$S/plugin" \
    --model haiku \
    --no-session-persistence \
    --permission-mode dontAsk \
    --allowedTools "Agent" "Bash(echo *)" \
    --output-format text \
    "Call the Agent tool exactly once with subagent_type \"$agent\"$extra and prompt \"run the probe\". Then print the subagent final reply verbatim and nothing else. If the call is refused, print the refusal text verbatim." \
    > "$S/out/$tag.reply.txt" 2> "$S/out/$tag.err.txt"
  echo "$tag exit=$?" >> "$S/out/summary.txt"
}
: > "$S/out/summary.txt"
run r1 spikeplug:probe ""
run r2 spikeplug:probe "probe-named"
run r3 spikeplug:probe-q ""
run r4 spikeplug:probe-q "probe-q-named"
echo done >> "$S/out/summary.txt"
