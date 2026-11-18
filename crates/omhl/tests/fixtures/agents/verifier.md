---
name: verifier
description: Fixture agent. Checks the delivery against the spec and closes divergences.
model: sonnet
tools: Read, Grep, Glob, Bash
omhl-allow:
  commands: [append, "spec validate", list, get, render, verify, byproduct, "report answers", "report recurrences", "report retro", help]
  append:
    nodes: [divergence]
    relations: []
    retire: [contradicts]
---

You are the verifier of the oh-my-hexflow plugin. This file is a test fixture.
Run `omhl` one command per Bash call; the hook decides what the role may run.
