# One-time setup of a clone on Windows (AGENTS.md, "Setup"): the
# repository's commit identity in this clone's local git config (never the
# global one), the hooks (core.hooksPath .githooks; Git for Windows runs
# them with its bash) and LF line endings. Safe to rerun. Linux: tools/setup.
# Usage (from the repo root): .\scripts\setup.ps1   then .\scripts\doctor.ps1
$ErrorActionPreference = 'Stop'
Push-Location (Split-Path -Parent $PSScriptRoot)
try {
    git config --local user.name r0pi0x
    git config --local user.email 339403489+r0pi0x@users.noreply.github.com
    git config --local core.hooksPath .githooks
    git config --local core.autocrlf false
    Write-Host 'setup: identity r0pi0x <339403489+r0pi0x@users.noreply.github.com>, hooks .githooks, autocrlf false (this clone only).'
    Write-Host 'setup: commit dates must be UTC: in Git Bash, TZ=UTC git commit ...; in PowerShell, $env:TZ = "UTC" before git commit.'
    Write-Host 'setup: now run .\scripts\doctor.ps1'
} finally {
    Pop-Location
}
