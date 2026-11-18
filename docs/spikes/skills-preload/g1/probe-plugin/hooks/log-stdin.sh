#!/bin/sh
# G1 probe: append the hook stdin to $SPIKE_LOG (no context injection). Always exits 0.
ev="$1"
in=$(cat)
printf '%s\t%s\n' "$ev" "$in" >> "${SPIKE_LOG:-<repo>/.omc/research/omc-port-g1/out/b.hooks.tsv}"
if [ "$ev" = "PreToolUse" ]; then
  aid=$(printf '%s' "$in" | jq -r '.agent_id // empty')
  tp=$(printf '%s' "$in" | jq -r '.transcript_path // empty')
  if [ -n "$aid" ] && [ -n "$tp" ]; then
    d1="${tp%.jsonl}/subagents"; d2=$(dirname "$tp")
    f=absent; for d in "$d1" "$d2"; do [ -f "$d/agent-$aid.meta.json" ] && f=found; done
    printf 'META\t%s\t%s\n' "$aid" "$f" >> "${SPIKE_LOG:-<repo>/.omc/research/omc-port-g1/out/b.hooks.tsv}"
  fi
fi
exit 0
