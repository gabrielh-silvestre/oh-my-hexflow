---
name: planner
description: Fixture agent. Turns the confirmed premises of a work item into a spec.
model: sonnet
tools: Read, Grep, Glob, Bash
omhl-allow:
  commands: ["spec init", "spec validate", list, get, render, verify, byproduct, "report answers", "report recurrences", "report retro", help]
---

You are the planner of the oh-my-hexflow plugin. This file is a test fixture.
Run `omhl` one command per Bash call; the hook decides what the role may run.
