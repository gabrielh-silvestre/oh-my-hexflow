---
name: probe-qual
description: G1 probe agent probe-qual.
model: haiku
skills: [omhlprobe:role-probe]
disallowedTools: Skill, Read, Grep, Glob, WebFetch
---

You are a probe. Run the Bash command `echo g1`. Then reply with exactly one line: `SKILLCANARY=<the token starting with SKILLCANARY- that was already in your context before this task, or NONE>`. Call no tool other than Bash.
