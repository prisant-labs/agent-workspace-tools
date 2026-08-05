"""Deterministically generate the synthetic reference-move fixtures (S-04 AC-62).

The original fixtures were two REAL Claude session transcripts (18.1 MB, unredacted) committed
to a public repository. This generator replaces them with synthetic files that preserve every
property the golden tests lock, and nothing else:

- exact anchored-rewrite totals under `build_path_rules(old, new)`:
    "cwd" field form      1467   (file1 227 + file2 1240)
    backslash prefix form  588   (file1  54 + file2  534)
    forward-slash form      27   (file2 only)
  => total 2082, asserted by crates/awt-core/tests/anchored_reference.rs
- preserved non-path mentions: `<project>@...` x10, `<project>_dev-...` x55
  (counts must merely SURVIVE the rewrite; the descriptor in move.json records them)
- line counts 329 and 2285, every line valid JSON (post-move verification parses each line)
- file names kept (session-UUID basenames), so the seed tests are unchanged

after/ mirrors are produced by applying the same three literal replacements in rule order,
mimicking `anchored_rewrite` exactly.

Deterministic by construction: no randomness, no timestamps. Re-running always produces
byte-identical output. Run from the repo root:

    python scripts/generate-reference-fixtures.py
"""

import json
import os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BASE = os.path.join(ROOT, "test", "fixtures", "reference-move")

# Fictional project identities. These are NOT real paths, deliberately: the fixtures once
# carried the real 2026-07-09 reference move, and while AC-62 replaced the file CONTENT with
# synthetic data it left the real project identity in place. Identity survives a content
# scrub - directory names, hash-derived suffixes, and worked examples in unit tests all
# outlive it - so the names themselves are synthetic too.
#
# The shape is load-bearing and must be preserved by any future rename:
#   - a space inside a path segment ("Sample Repos"), which exercises path encoding
#   - a hyphenated project name, so the lookalike strings below are near-misses
#   - different parent segments for src and dst, so the move is not a simple leaf rename
OLD = r"E:\Projects\Sample Repos\demo-notes-editor"
NEW = r"E:\Projects\demo-labs\demo-notes-editor-pro"

# The bare project name, used for the preserved near-miss strings. Derived rather than
# repeated so a rename cannot leave the lookalikes pointing at the old identity.
OLD_NAME = OLD.rsplit("\\", 1)[1]

ESC_OLD = OLD.replace("\\", "\\\\")  # JSON-escaped, as stored in file bytes
FWD_OLD = OLD.replace("\\", "/")

# The three anchored literals, byte-for-byte what build_path_rules() searches for.
R1 = f'"cwd":"{ESC_OLD}"'
R2 = f"{ESC_OLD}\\\\"  # escaped path + escaped separator
R3 = f"{FWD_OLD}/"


def jline(obj):
    return json.dumps(obj, separators=(",", ":"))


def build_file(uuid, n_cwd, n_prefix, n_fwd, n_at, n_dev, n_lines):
    lines = []
    # R1: exact cwd fields. json.dumps would re-escape, so these lines are assembled by hand
    # around the literal; each is still valid JSON.
    for i in range(n_cwd):
        lines.append('{"type":"user","cwd":"' + ESC_OLD + f'","uuid":"{uuid}-c{i:04}"}}')
    # R2: backslash path-prefix occurrences (a file path under the project).
    for i in range(n_prefix):
        lines.append('{"type":"tool","file":"' + ESC_OLD + "\\\\src\\\\mod_" + f'{i:04}.rs"}}')
    # R3: forward-slash prefix occurrences (a URL-ish reference).
    for i in range(n_fwd):
        lines.append('{"type":"link","url":"' + FWD_OLD + f'/docs/page_{i:04}.html"}}')
    # Preserved near-misses: the project NAME without its path. These must come through the
    # rewrite byte-identical; they are why "surgical" is a testable claim.
    for i in range(n_at):
        lines.append(jline({"type": "note", "text": f"bump {OLD_NAME}@0.2.{i} in package.json"}))
    for i in range(n_dev):
        lines.append(jline({"type": "note", "text": f"checked out branch {OLD_NAME}_dev-{i:03}"}))
    # Filler to the exact historical line count.
    filler = n_lines - len(lines)
    assert filler >= 0, f"{uuid}: over line budget by {-filler}"
    for i in range(filler):
        lines.append(jline({"type": "progress", "seq": i, "note": "synthetic filler"}))
    text = "\n".join(lines) + "\n"
    # Self-check the anchored counts before writing anything.
    assert text.count(R1) == n_cwd, (uuid, "R1", text.count(R1), n_cwd)
    assert text.count(R2) == n_prefix, (uuid, "R2", text.count(R2), n_prefix)
    assert text.count(R3) == n_fwd, (uuid, "R3", text.count(R3), n_fwd)
    assert text.count("\n") == n_lines
    for line in text.splitlines():
        json.loads(line)  # every line must parse
    return text


def rewrite(text):
    """Mirror anchored_rewrite's sequential literal replacement, rule order R1, R2, R3."""
    esc_new = NEW.replace("\\", "\\\\")
    fwd_new = NEW.replace("\\", "/")
    text = text.replace(R1, f'"cwd":"{esc_new}"')
    text = text.replace(R2, f"{esc_new}\\\\")
    text = text.replace(R3, f"{fwd_new}/")
    return text


FILES = [
    # (uuid-basename, cwd, prefix, fwd, at, dev, lines)
    ("22b2362e-e4ef-4042-9b01-e3cba5719590", 227, 54, 0, 3, 20, 329),
    ("28fd093e-f5ef-4dc7-af16-ea415c1840f7", 1240, 534, 27, 7, 35, 2285),
]

OLD_ENC = "E--Projects-Sample-Repos-demo-notes-editor"
NEW_ENC = "E--Projects-demo-labs-demo-notes-editor-pro"

total = 0
for uuid, c, p, f, at, dev, lines in FILES:
    text = build_file(uuid, c, p, f, at, dev, lines)
    total += c + p + f
    before = os.path.join(BASE, "before", "projects", OLD_ENC)
    after = os.path.join(BASE, "after", "projects", NEW_ENC)
    os.makedirs(before, exist_ok=True)
    os.makedirs(after, exist_ok=True)
    with open(os.path.join(before, f"{uuid}.jsonl"), "w", encoding="utf-8", newline="\n") as fh:
        fh.write(text)
    with open(os.path.join(after, f"{uuid}.jsonl"), "w", encoding="utf-8", newline="\n") as fh:
        fh.write(rewrite(text))
    print(f"{uuid}: {lines} lines, R1={c} R2={p} R3={f} @={at} dev={dev}")

assert total == 1467 + 588 + 27, total
print(f"anchored total: {total} (locked by anchored_reference.rs)")
