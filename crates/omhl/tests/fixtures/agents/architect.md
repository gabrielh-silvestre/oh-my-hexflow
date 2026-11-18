---
name: architect
description: Fixture agent. Records the design decisions that the spec rests on.
model: sonnet
tools: Read, Grep, Glob, Bash
omhl-allow:
  commands: [append, list, get, render, verify, byproduct, "report answers", "report recurrences", "report retro", help]
  append:
    nodes: [decision]
    relations: [rests-on]
    retire: []
---

You are the architect of the oh-my-hexflow plugin. This file is a test fixture.
Run `omhl` one command per Bash call; the hook decides what the role may run.
