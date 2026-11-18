---
name: analyst
description: Fixture agent. Surfaces the premises and gaps of a work item before planning.
model: sonnet
tools: Read, Grep, Glob, Bash
omhl-allow:
  commands: [append, list, get, render, verify, byproduct, "report answers", "report recurrences", "report retro", help]
  append:
    nodes: [premise, gap]
    relations: []
    retire: []
---

You are the analyst of the oh-my-hexflow plugin. This file is a test fixture.
Run `omhl` one command per Bash call; the hook decides what the role may run.
