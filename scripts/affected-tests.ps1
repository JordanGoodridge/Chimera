# Windows entry point for repo_checks' affected-test selection, also used by
# .githooks/pre-commit. Runs the affected packages' fast tests and repo_checks.
# Examples (paths for -Files are relative to the repository root):
#   .\scripts\affected-tests.ps1 -Files chimera/client/tests/it/metrics.rs
#   .\scripts\affected-tests.ps1 -Staged
#   .\scripts\affected-tests.ps1 -Base upstream/main
#   .\scripts\affected-tests.ps1 -Staged -BuildOnly
# -Base considers committed changes only. Use -Files or -Staged for local edits.
# Unlike tools/agent-env.sh this does not configure cgroups or a jobserver.
[CmdletBinding(DefaultParameterSetName = 'Files')]
param(
    [Parameter(ParameterSetName = 'Files', Mandatory = $true)]
    [string[]]$Files,
    [Parameter(ParameterSetName = 'Staged', Mandatory = $true)]
    [switch]$Staged,
    [Parameter(ParameterSetName = 'Base', Mandatory = $true)]
    [string]$Base,
    [switch]$BuildOnly,
    [switch]$Nextest,
    [switch]$Direct,
    [string]$Remote = 'origin'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
if (-not (Test-Path $cargo)) { $cargo = 'cargo' }
$originalPath = $env:PATH

Push-Location $root
try {
    if ($BuildOnly -and $Nextest) { throw 'Choose either -BuildOnly or -Nextest' }
    if ([IO.Path]::IsPathRooted($cargo)) {
        $env:PATH = (Split-Path -Parent $cargo) + ';' + $env:PATH
    }
    $format = if ($Nextest) { 'nextest' } else { 'cargo' }
    # `cargo run` would use target/, then the test command with --features dev
    # (and more dependency features) would relink the same helper binary.
    # Give selection its own cache so both sides stay warm.
    $originalTargetDir = $env:CARGO_TARGET_DIR
    $env:CARGO_TARGET_DIR = Join-Path $root 'target\affected-selector'
    $select = @('run', '-q', '-p', 'repo_checks', '--bin', 'affected', '--', '--format', $format, '--remote', $Remote)
    if ($Nextest) { $select += '--skip-heavy' }
    if ($Direct) { $select += '--direct' }
    if ($Staged) {
        $select += '--staged'
    } elseif ($PSCmdlet.ParameterSetName -eq 'Base') {
        $select += @('--base', $Base)
    } else {
        $select += @('--files') + $Files
    }

    # Windows PowerShell treats native stderr as errors under Stop; Cargo
    # writes normal progress there. Check native exit codes explicitly.
    $ErrorActionPreference = 'Continue'
    try {
        $selected = @(& $cargo @select)
        $code = $LASTEXITCODE
    } finally {
        if ($null -eq $originalTargetDir) {
            Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
        } else {
            $env:CARGO_TARGET_DIR = $originalTargetDir
        }
        $ErrorActionPreference = 'Stop'
    }
    if ($code -ne 0) { throw "affected selection failed (exit $code)" }
    $selected = @($selected | Where-Object { $_ -and $_.Trim().Length -gt 0 })
    if ($selected.Count -eq 0) { throw 'affected selected nothing' }

    Write-Host "affected: $($selected -join ' ')"
    $testArgs = @('test') + $selected + @('--features', 'dev')
    if ($Nextest) {
        $testArgs = @('nextest', 'run') + $selected + @('--features', 'dev', '--no-fail-fast')
    } elseif ($BuildOnly) {
        $testArgs += '--no-run'
    } else {
        $testArgs += @('--no-fail-fast', '--', '--skip', 'heavy::')
    }
    $ErrorActionPreference = 'Continue'
    & $cargo @testArgs
    $code = $LASTEXITCODE
} finally {
    $env:PATH = $originalPath
    Pop-Location
}
exit $code
