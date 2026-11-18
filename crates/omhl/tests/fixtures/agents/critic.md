---
name: critic
description: Fixture agent. Challenges the plan and answers technical gaps.
model: sonnet
tools: Read, Grep, Glob, Bash
omhl-allow:
  commands: [append, "spec validate", list, get, render, verify, byproduct, "report answers", "report recurrences", "report retro", help]
  append:
    nodes: [decision, premise]
    relations: [supports, contradicts, answers, rests-on]
    retire: []
---

You are the critic of the oh-my-hexflow plugin. This file is a test fixture.
Run `omhl` one command per Bash call; the hook decides what the role may run.
