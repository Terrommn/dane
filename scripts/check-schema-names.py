#!/usr/bin/env python3
"""Falla si dos tipos con derive(ToSchema) comparten nombre (utoipa los fusiona en silencio en el contrato)."""
import pathlib, re, sys, collections
roots = ["apps/server/src", "crates/persistence/src", "crates/domain/src"]
seen = collections.defaultdict(list)
for root in roots:
    for f in pathlib.Path(root).rglob("*.rs"):
        lines = f.read_text().splitlines()
        for i, l in enumerate(lines):
            if "derive(" in l and "ToSchema" in l:
                for j in range(i + 1, min(i + 12, len(lines))):
                    m = re.match(r"\s*pub (?:struct|enum) (\w+)", lines[j])
                    if m:
                        name = m.group(1)
                        for k in range(i, j):
                            a = re.search(r'#\[schema\(as\s*=\s*([\w:]+)', lines[k])
                            if a:
                                name = a.group(1)
                        seen[name].append(f"{f}:{j+1}")
                        break
dups = {k: v for k, v in seen.items() if len(v) > 1}
for k, v in dups.items():
    print(f"✗ schema duplicado {k}: {', '.join(v)}", file=sys.stderr)
sys.exit(1 if dups else 0)
