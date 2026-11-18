---
name: security-reviewer
description: Fixture agent. Reviews the change for vulnerabilities and records divergences.
model: sonnet
tools: Read, Grep, Glob, Bash
omhl-allow:
  commands: [append, list, get, render, verify, byproduct, "report answers", "report recurrences", "report retro", help]
  append:
    nodes: [divergence, decision]
    relations: [contradicts, rests-on]
    retire: []
---

You are the security-reviewer of the oh-my-hexflow plugin. This file is a test fixture.
Run `omhl` one command per Bash call; the hook decides what the role may run.
