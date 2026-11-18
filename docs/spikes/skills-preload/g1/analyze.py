#!/usr/bin/env python3
"""G1 reader: per subagent, what the hooks saw (agent_type), how the main session called it, META, and where the canary entered.
usage: analyze.py <hooks.tsv> <canary-file> [copy-dir-prefix]  (reads ~/.claude/projects/**/subagents/agent-<id>.{meta.json,jsonl})"""
import sys, json, glob, os, shutil
log, canf = sys.argv[1], sys.argv[2]
can = open(canf).read().strip()
home = os.path.expanduser("~/.claude/projects")
calls, subs, meta = [], {}, {}
for l in open(log):
    p = l.rstrip("\n").split("\t")
    if p[0] == "META": meta[p[1]] = p[2]; continue
    try: o = json.loads(p[1])
    except Exception: continue
    aid = o.get("agent_id")
    if p[0] == "PreToolUse" and not aid and o.get("tool_name") in ("Agent", "Task", "Skill"):
        ti = o.get("tool_input") or {}
        calls.append(dict(tool=o["tool_name"], st=ti.get("subagent_type"), name=ti.get("name"), skill=ti.get("skill"), prompt_canary=can in (ti.get("prompt") or "")))
    if aid:
        s = subs.setdefault(aid, dict(start=None, pre_types=set(), tools=[]))
        if p[0] == "SubagentStart": s["start"] = o.get("agent_type")
        if p[0] == "PreToolUse": s["pre_types"].add(o.get("agent_type")); s["tools"].append(o.get("tool_name"))
print("MAIN SESSION CALLS:"); [print("  ", c) for c in calls]
for aid, s in subs.items():
    mp = glob.glob(f"{home}/**/subagents/agent-{aid}*.meta.json", recursive=True)
    tp = glob.glob(f"{home}/**/subagents/agent-{aid}*.jsonl", recursive=True)
    m = json.load(open(mp[0])) if mp else {}
    marker = first_tool = None; skilltools = []
    if tp:
        for i, l in enumerate(open(tp[0])):
            r = json.loads(l); st = json.dumps(r)
            if marker is None and r.get("type") == "user" and "<command-message>" in st and "role-probe" in st: marker = i
            c = (r.get("message") or {}).get("content")
            if isinstance(c, list):
                for b in c:
                    if isinstance(b, dict) and b.get("type") == "tool_use":
                        first_tool = i if first_tool is None else first_tool
                        if b.get("name") in ("Skill", "Read", "Grep", "Glob"): skilltools.append(b["name"])
    print(f"SUBAGENT {aid[:8]}: SubagentStart.agent_type={s['start']!r} PreToolUse.agent_type={sorted(map(str,s['pre_types']))} tools={s['tools']} META={meta.get(aid)}")
    print(f"   meta.json: agentType={m.get('agentType')!r} customAgentType={m.get('customAgentType')!r} name={m.get('name')!r} taskKind={m.get('taskKind')!r} shape={m.get('requestShape')!r}")
    print(f"   transcript: preload marker at rec {marker}, first tool_use at rec {first_tool}, Skill/Read/Grep/Glob tool_use={skilltools}")
