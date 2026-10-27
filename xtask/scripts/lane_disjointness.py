"""Verify lane file sets in a tasks.md are pairwise disjoint and avoid frozen paths.

Usage: lane_disjointness.py <path/to/specs/NNN-name/tasks.md>
"""
import itertools
import os
import re
import sys

tasks_path = sys.argv[1]
feature_dir = os.path.dirname(os.path.normpath(tasks_path)).replace(os.sep, "/")
FROZEN = (feature_dir + "/contracts/", "crates/mouser-proto/", "crates/mouser-platform/",
          "crates/mouser-testkit/", "crates/mouser-core/tests/features/",
          "apps/mouser-service-win/tests/features/",
          "crates/mouser-platform-windows/tests/features/")
ROOT_FROZEN = ("Cargo.toml", "Cargo.lock")
REPO_ROOTS = ("crates/", "apps/", "xtask/", "specs/", ".github/")
text = open(tasks_path).read()
start = re.search(r"^## PARALLEL GROUP", text, re.M)
end = re.search(r"^## Phase ", text[start.end():], re.M)
group = text[start.end():start.end() + end.start()]
lanes = {}
for block in re.split(r"^### Lane: ", group, flags=re.M)[1:]:
    name, body = block.split("\n", 1)
    paths = set()
    for line in body.splitlines():
        if not re.match(r"- \[[ xX]\] T\d+", line):
            continue
        for p in re.findall(r"`([^`\s]+)`", line):
            if p.startswith(REPO_ROOTS):
                paths.add(p.rstrip("/"))
            elif "/" not in p and os.path.exists(p):
                paths.add(p)
    lanes[name.strip()] = paths

ok = True
for name, paths in lanes.items():
    print(f"lane {name}: {len(paths)} paths")
    for p in sorted(paths):
        if p.startswith(FROZEN) or (p + "/").startswith(FROZEN) or p in ROOT_FROZEN:
            print(f"  FROZEN PATH in {name}: {p}")
            ok = False
for (a, pa), (b, pb) in itertools.combinations(lanes.items(), 2):
    both = pa & pb
    if both:
        print(f"OVERLAP {a} / {b}: {sorted(both)}")
        ok = False
print("RESULT:", "disjoint" if ok else "NOT disjoint")
sys.exit(0 if ok else 1)
