<!--
Read CONTRIBUTING.md first. This project edits users' Claude Code state, so correctness and
safety gate everything else.
-->

## What this changes

<!-- One or two sentences. Link the issue or acceptance criterion if there is one. -->

## Why

<!-- The problem being solved, not a restatement of the diff. -->

## Verification

Paste the actual output, not a claim that it passed.

- [ ] `cargo test --workspace`
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets -- -D warnings`

```
<!-- paste results here -->
```

## Checklist

- [ ] A failing test was written first, and it fails without this change.
- [ ] Any new fixture is referenced by at least one test. An unreferenced fixture reads as
      coverage that does not exist.
- [ ] Tests covering a rewrite assert on **raw file bytes**, not on parsed values or `plan`
      output. A parsed-value assertion can pass while the byte splice is impossible.
- [ ] No em-dashes or en-dashes anywhere, including code comments and this description.
- [ ] Every new document has a row in `docs/index.md` and an entry in `docs/CHANGELOG.md`.

## Invariants

Confirm this change preserves them, or explain the trade:

- [ ] No network or LLM calls in the migration path.
- [ ] Deterministic: no wall-clock or randomness affecting output.
- [ ] Backup before write, verify after, recoverable by `rollback`.
- [ ] Hard-fails on an unrecognized store shape rather than guessing.
- [ ] Never rewrites another project's transcripts.

## User-visible changes

<!--
If this changes CLI flags, output, or exit codes, list them and note which docs were updated:
docs/reference/commands.md, docs/quickstart.md, docs/troubleshooting.md, docs/recipes.md.
Otherwise write "none".
-->
