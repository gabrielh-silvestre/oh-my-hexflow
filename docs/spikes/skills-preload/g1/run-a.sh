#!/bin/sh
# G1 part A: six headless runs (haiku). Same shape as spike 1, but WITHOUT --no-session-persistence so subagent transcripts exist.
R=<repo>/.omc/research/omc-port-g1
W=<scratchpad>/g1work
mkdir -p "$W"; cd "$W" || exit 1
: > "$R/out/summary-a.txt"
run() { # tag agent name
  export SPIKE_LOG="$R/out/$1.hooks.tsv"; : > "$SPIKE_LOG"
  if [ -n "$3" ]; then extra=" and name \"$3\""; else extra=" and NO name parameter"; fi
  timeout 240 claude -p --plugin-dir "$R/probe-plugin" --model haiku --permission-mode dontAsk \
    --allowedTools "Agent" "Bash(echo *)" --output-format text \
    "Call the Agent tool exactly once with subagent_type \"$2\"$extra and prompt \"run the probe\". Then print the subagent final reply verbatim and nothing else. If the call is refused or fails, print the error text verbatim." \
    > "$R/out/$1.reply.txt" 2> "$R/out/$1.err.txt"
  echo "$1 $2 name=[$3] exit=$?" >> "$R/out/summary-a.txt"
}
run a1 omhlprobe:probe-bare ""
run a2 omhlprobe:probe-qual ""
run a3 omhlprobe:probe-none ""
run a4 omhlprobe:probe-missing ""
run a6 omhlprobe:probe-qual-skill ""
run a5 omhlprobe:probe-qual "g1-a-named"
{ claude --version; date -u +%FT%TZ; } > "$R/VERSION.txt"
echo done >> "$R/out/summary-a.txt"
