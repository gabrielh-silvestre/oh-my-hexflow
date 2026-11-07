"""Filters the official JSON-Schema-Test-Suite (draft2020-12) down to the subset the hexlog catalog supports.

Usage: python3 -I filter.py <suite-checkout> <out-dir>
  <suite-checkout>  checkout of json-schema-org/JSON-Schema-Test-Suite at the pinned commit
  <out-dir>         receives the filtered files, compact JSON, same file names as the originals

A group is kept when every schema in it uses only whitelisted keywords, every $ref is internal and
resolves, $schema is only at the root with the 2020-12 URI, and every number in the schema is a safe
integer. A test is kept when its data has no number outside the safe-integer guard.
The suite JSON is treated as data only: this script never executes anything from it.
"""
import glob
import json
import os
import shutil
import sys
import urllib.parse

SAFE = 2**53 - 1
ALLOWED = {"type", "enum", "const", "minimum", "maximum", "minLength", "maxLength", "minItems",
           "maxItems", "required", "properties", "additionalProperties", "items", "allOf", "anyOf", "oneOf",
           "not", "$defs", "$ref", "title", "description", "default", "examples", "$comment",
           "deprecated", "readOnly", "writeOnly", "$schema"}


def safe(x):
    if isinstance(x, bool):
        return True
    if isinstance(x, float):
        return False
    if isinstance(x, int):
        return abs(x) <= SAFE
    return True


def nums_ok(v):
    if isinstance(v, dict):
        return all(nums_ok(x) for x in v.values())
    if isinstance(v, list):
        return all(nums_ok(x) for x in v)
    return safe(v)


def resolve(root, frag):
    cur = root
    if urllib.parse.unquote(frag) == "":
        return cur
    for tok in urllib.parse.unquote(frag)[1:].split("/"):
        tok = tok.replace("~1", "/").replace("~0", "~")
        if isinstance(cur, dict) and tok in cur:
            cur = cur[tok]
        elif isinstance(cur, list) and tok.isdigit() and int(tok) < len(cur):
            cur = cur[int(tok)]
        else:
            return None
    return cur


def walk(s, root, top=False):
    if isinstance(s, bool):
        return True
    if not isinstance(s, dict):
        return False
    for k, v in s.items():
        if k not in ALLOWED:
            return False
        if k == "$schema" and (not top or v != "https://json-schema.org/draft/2020-12/schema"):
            return False
        if k in ("enum", "const") and not nums_ok(v):
            return False
        if k in ("minimum", "maximum", "minLength", "maxLength", "minItems", "maxItems") and not safe(v):
            return False
        if k in ("properties", "$defs") and not all(walk(x, root) for x in v.values()):
            return False
        if k in ("additionalProperties", "items", "not") and not walk(v, root):
            return False
        if k in ("allOf", "anyOf", "oneOf") and not (v and all(walk(x, root) for x in v)):
            return False
        if k == "$ref" and not (isinstance(v, str) and (v == "#" or v.startswith("#/"))
                                and resolve(root, v[1:]) is not None):
            return False
    return True


def main(src, out):
    shutil.rmtree(out + ".tmp", ignore_errors=True)
    os.makedirs(out + ".tmp")
    files = groups_total = tests_total = 0
    for path in sorted(glob.glob(os.path.join(src, "tests", "draft2020-12", "*.json"))):
        with open(path, encoding="utf-8") as handle:
            groups = json.load(handle)
        keep = []
        for group in groups:
            if not walk(group["schema"], group["schema"], True):
                continue
            tests = [t for t in group["tests"] if nums_ok(t["data"])]
            if tests:
                keep.append({**group, "tests": tests})
        if keep:
            name = os.path.basename(path)
            with open(os.path.join(out + ".tmp", name), "w", encoding="utf-8") as handle:
                json.dump(keep, handle, separators=(",", ":"), ensure_ascii=False)
            count = sum(len(g["tests"]) for g in keep)
            print(f"{name}\t{len(keep)}\t{count}")
            files += 1
            groups_total += len(keep)
            tests_total += count
    os.makedirs(out, exist_ok=True)
    for old in glob.glob(os.path.join(out, "*.json")):
        os.remove(old)
    for new in glob.glob(os.path.join(out + ".tmp", "*.json")):
        shutil.move(new, os.path.join(out, os.path.basename(new)))
    shutil.rmtree(out + ".tmp")
    print(f"TOTAL\t{files}\t{groups_total}\t{tests_total}")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
