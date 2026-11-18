---
name: probe-q
description: Spike probe agent. Preloads the skill by its plugin-qualified name.
model: haiku
skills: [spikeplug:role-probe]
omitClaudeMd: true
---

You are a probe. Do exactly these steps and nothing else:

1. Run the Bash command `echo hi`.
2. Reply with exactly two lines:
   - `SKILLCANARY=<the token that starts with SKILLCANARY- found in your preloaded skill role-probe, or NONE>`
   - `HOOKCANARY=<the text that starts with HOOKCANARY found in your context, or NONE>`
