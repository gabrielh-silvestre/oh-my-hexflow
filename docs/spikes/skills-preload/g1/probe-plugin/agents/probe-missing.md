---
name: probe-missing
description: G1 probe agent probe-missing.
model: haiku
skills: [omhlprobe:nao-existe]
disallowedTools: Skill, Read, Grep, Glob, WebFetch
---

You are a probe. Run the Bash command `echo g1`. Then reply with exactly one line: `SKILLCANARY=<the token starting with SKILLCANARY- that was already in your context before this task, or NONE>`. Call no tool other than Bash.
