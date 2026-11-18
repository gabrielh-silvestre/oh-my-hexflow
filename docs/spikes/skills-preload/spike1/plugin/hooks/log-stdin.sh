#!/bin/sh
# Spike only: append the hook stdin to $SPIKE_LOG; on SubagentStart also inject the agent_type back.
ev="$1"
in=$(cat)
printf '%s\t%s\n' "$ev" "$in" >> "${SPIKE_LOG:-/dev/null}"
if [ "$ev" = "SubagentStart" ]; then
  at=$(printf '%s' "$in" | jq -r '.agent_type // "none"')
  printf '{"hookSpecificOutput":{"hookEventName":"SubagentStart","additionalContext":"HOOKCANARY agent_type=%s"}}' "$at"
fi
exit 0
