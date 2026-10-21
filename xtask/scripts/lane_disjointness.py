"""T049: verify lane file sets in tasks.md are pairwise disjoint and avoid frozen paths."""
import re, sys, itertools

FROZEN = ("specs/001-mouser-v1/contracts/", "crates/mouser-proto/", "crates/mouser-platform/",
          "crates/mouser-testkit/", "crates/mouser-core/tests/features/")
REPO_ROOTS = ("crates/", "apps/", "xtask/", "specs/", ".github/")
text = open(sys.argv[1]).read()
group = text.split("## PARALLEL GROUP", 1)[1].split("\n## Phase 54", 1)[0]
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
    lanes[name.strip()] = paths

ok = True
for name, paths in lanes.items():
    print(f"lane {name}: {len(paths)} paths")
    for p in sorted(paths):
        if p.startswith(FROZEN) or (p + "/").startswith(FROZEN):
            print(f"  FROZEN PATH in {name}: {p}"); ok = False
for (a, pa), (b, pb) in itertools.combinations(lanes.items(), 2):
    both = pa & pb
    if both:
        print(f"OVERLAP {a} / {b}: {sorted(both)}"); ok = False
print("RESULT:", "disjoint" if ok else "NOT disjoint")
sys.exit(0 if ok else 1)
