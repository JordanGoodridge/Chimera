# The Windows equivalent of .githooks/pre-commit's test selection: ask
# repo_checks' `affected` which packages a change can affect, then build
# or run only those (plus repo_checks, whose checks always run).
#
#   .\scripts\affected-tests.ps1 -Staged              # what is staged
#   .\scripts\affected-tests.ps1 -Base upstream/main  # this branch
#   .\scripts\affected-tests.ps1 -Files source/formats/src/lib.rs
#   .\scripts\affected-tests.ps1 -Staged -BuildOnly   # pre-commit's build half
#   .\scripts\affected-tests.ps1 -Staged -Direct      # only the changed crates
#   .\scripts\affected-tests.ps1 -Staged -Nextest     # cargo nextest run
#
# `--features dev` is accepted because `affected` always includes
# repo_checks, which has an empty `dev` feature for exactly this.
[CmdletBinding(DefaultParameterSetName = 'Base')]
param(
    [Parameter(ParameterSetName = 'Base')]
    [string]$Base = 'upstream/main',
    [Parameter(ParameterSetName = 'Files', Mandatory = $true)]
    [string[]]$Files,
    [Parameter(ParameterSetName = 'Staged', Mandatory = $true)]
    [switch]$Staged,
    [switch]$BuildOnly,
    [switch]$Nextest,
    [switch]$Direct,
    [string]$Remote = 'origin'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
if (-not (Test-Path $cargo)) { $cargo = 'cargo' }
$env:PATH = (Split-Path -Parent $cargo) + ';' + $env:PATH

$select = @('run', '-q', '-p', 'repo_checks', '--bin', 'affected', '--', '--format', 'cargo', '--remote', $Remote)
if ($Staged) {
    $select += '--staged'
} elseif ($Files) {
    $select += '--files'
    $select += $Files
} else {
    $select += '--base'
    $select += $Base
}
if ($Direct) { $select += '--direct' }

$eap = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
try {
    $lines = & $cargo @select
    $code = $LASTEXITCODE
} finally {
    $ErrorActionPreference = $eap
}
if ($code -ne 0) { throw "affected failed (exit $code)" }
$selected = @($lines | Where-Object { $_ -and $_.Trim().Length -gt 0 })
if ($selected.Count -eq 0) { throw 'affected selected nothing' }

Write-Host "affected: $($selected -join ' ')"
$testArgs = @('test') + $selected + @('--features', 'dev')
if ($Nextest) {
    $testArgs = @('nextest', 'run') + $selected + @('--features', 'dev', '-E', 'not test(/^heavy::/)')
} elseif ($BuildOnly) {
    $testArgs += '--no-run'
} else {
    $testArgs += @('--no-fail-fast', '--', '--skip', 'heavy::')
}
& $cargo @testArgs
exit $LASTEXITCODE
