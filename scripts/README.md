# scripts/

Maintainer and user helper scripts. These are conveniences around the CLI, never part of the
migration path: no script here is required to use `awt`, and the engine has no knowledge of them.

PowerShell 5.1 and PowerShell 7+ are both supported. Run from the repository root.

---

## `new-scratch-home.ps1`

Creates a disposable copy of a Claude home so you can run `awt` against it with `--home`.

```powershell
.\scripts\new-scratch-home.ps1 -Destination "E:\_temp\awt-practice"
```

| Parameter | Required | Default | Description |
|---|---|---|---|
| `-Destination` | yes | | Directory to create. Will contain `.claude\` and `.claude.json`. |
| `-SourceHome` | no | `$env:USERPROFILE` | The home to copy from. |
| `-Force` | no | off | Overwrite a non-empty destination. Refused without this. |

**Why it exists.** A Claude home is two things - the `.claude\` directory *and* the sibling
`.claude.json` file - and copying only the directory produces a scratch home that looks fine and
silently lacks every `projects{}` key and `githubRepoPaths` entry. Tests run against it then pass
for the wrong reason. The script copies both halves, and it is the required first step of the
[manual acceptance run](../docs/acceptance-run.md).

**What it protects against.**

- Copying a directory into its own subtree, which makes `robocopy` recurse into the growing copy.
  Refused outright.
- Silently overwriting an existing scratch copy. Refused unless `-Force`.
- Misreading `robocopy`'s exit status. `robocopy` returns a bit-field where 0-7 all mean success,
  so a naive `if ($LASTEXITCODE -ne 0)` treats a perfectly good copy as a failure. The script
  fails only on 8 and above, and exits 0 explicitly.

**It never writes to the source home.** Copy direction is one-way by construction.

**Runtime.** Roughly 2-4 minutes for a 3 GB home. Transcripts dominate the size.

**Cleaning up.** Nothing tracks scratch homes, so delete them yourself:

```powershell
Remove-Item -Recurse -Force "E:\_temp\awt-practice"
```

### Same-volume note

`awt apply` performs a real folder move of `--src` to `--dst`, and v1.0 refuses cross-volume
moves. The scratch home itself may live on any drive - it is `--src` and `--dst` that must share
one. A scratch home on `E:` testing a move between two `C:` paths is fine.

---

## Adding a script here

Keep the migration path free of scripts: anything that edits Claude state belongs in
`awt-core` behind the `FileSystem` trait where it can be tested against an in-memory filesystem.
Scripts are for setup, inspection, and release chores.

Every script needs comment-based help (`.SYNOPSIS`, `.DESCRIPTION`, `.PARAMETER`, `.EXAMPLE`) so
`Get-Help .\scripts\<name>.ps1 -Full` works, a row in this file, a row in
[`docs/index.md`](../docs/index.md), and an entry in `docs/CHANGELOG.md`.
