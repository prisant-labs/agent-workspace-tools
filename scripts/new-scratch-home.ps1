<#
.SYNOPSIS
    Create a scratch copy of a Claude home for safe testing of awt.

.DESCRIPTION
    awt edits state under a Claude home (`.claude/` plus the sibling `.claude.json`).
    Every awt command accepts `--home <dir>` to point at a different home, so the safe
    way to try the tool - and the required setup for the manual acceptance run - is to
    copy your real Claude home somewhere disposable and point awt at the copy.

    This script does that copy correctly: it takes both halves of the home (the
    directory and the sibling JSON file), refuses the footguns (copying into itself,
    silently overwriting an existing scratch copy), and prints the exact `--home`
    value to use afterwards.

    It only ever reads the source home. It never writes to it.

.PARAMETER Destination
    Directory to create the scratch home in. It will contain `.claude\` and
    `.claude.json`. Created if it does not exist.

.PARAMETER SourceHome
    The home directory to copy FROM. Defaults to $env:USERPROFILE (your live home).

.PARAMETER Force
    Overwrite a non-empty destination. Without this, a non-empty destination is refused.

.EXAMPLE
    .\scripts\new-scratch-home.ps1 -Destination "E:\Projects\_temp\awt-acceptance"

    Copies your live Claude home to the scratch location, then tells you to run
    `awt list --home "E:\Projects\_temp\awt-acceptance"`.

.NOTES
    Same-volume note: `awt apply` performs a real folder move of --src to --dst, and
    v1.0 refuses cross-volume moves. The scratch home itself may live on any volume;
    it is --src and --dst that must share one.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Destination,

    [string]$SourceHome = $env:USERPROFILE,

    [switch]$Force
)

$ErrorActionPreference = 'Stop'

$srcDir  = Join-Path $SourceHome '.claude'
$srcJson = Join-Path $SourceHome '.claude.json'

if (-not (Test-Path $srcDir)) {
    throw "Source Claude home not found: $srcDir"
}

# Resolve to full paths so the containment check below is meaningful.
$srcDirFull = (Resolve-Path $srcDir).Path
$dstFull    = [System.IO.Path]::GetFullPath($Destination)

# Refuse to copy a directory into itself or into its own subtree: robocopy would
# recurse into the growing copy.
if ($dstFull.StartsWith($srcDirFull, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Destination '$dstFull' is inside the source home '$srcDirFull'. Choose a destination outside it."
}

if ((Test-Path $dstFull) -and (Get-ChildItem $dstFull -Force | Select-Object -First 1)) {
    if (-not $Force) {
        throw "Destination '$dstFull' exists and is not empty. Pass -Force to overwrite, or pick another path."
    }
    Write-Host "Destination is not empty; -Force given, continuing." -ForegroundColor Yellow
}

New-Item -ItemType Directory -Force -Path $dstFull | Out-Null
$dstDir = Join-Path $dstFull '.claude'

Write-Host "Copying $srcDirFull"
Write-Host "     -> $dstDir"
Write-Host "(3+ GB is normal; this takes a few minutes.)"

# /E copy subdirectories including empty ones. /MT multithreaded. /R:1 /W:1 so a
# transiently locked file costs a second, not 30. /XJ skips junctions, which would
# otherwise let the copy escape the tree.
robocopy $srcDirFull $dstDir /E /MT:16 /R:1 /W:1 /XJ /NFL /NDL /NJH /NJS | Out-Null

# robocopy uses a bit-field exit code: 0-7 are success (files copied, extras present,
# and so on); 8 and above mean at least one file genuinely failed to copy.
$rc = $LASTEXITCODE
if ($rc -ge 8) {
    throw "robocopy failed with exit code $rc. The scratch copy is incomplete; do not use it."
}

if (Test-Path $srcJson) {
    Copy-Item $srcJson (Join-Path $dstFull '.claude.json') -Force
} else {
    Write-Host "Note: no .claude.json beside the source home; skipping it." -ForegroundColor Yellow
}

$stat = Get-ChildItem $dstDir -Recurse -File -ErrorAction SilentlyContinue |
        Measure-Object -Property Length -Sum

Write-Host ""
Write-Host "Scratch home ready." -ForegroundColor Green
Write-Host ("  files: {0:N0}   size: {1:N2} GB" -f $stat.Count, ($stat.Sum / 1GB))
Write-Host ""
Write-Host "Point awt at it with:"
Write-Host "  awt list --home `"$dstFull`"" -ForegroundColor Cyan
Write-Host ""
Write-Host "Delete it when you are done:"
Write-Host "  Remove-Item -Recurse -Force `"$dstFull`""

# Exit 0 explicitly: robocopy left a non-zero success code in $LASTEXITCODE.
exit 0
