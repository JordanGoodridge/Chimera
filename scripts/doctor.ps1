# Is this clone and PC ready to build, test and commit? (AGENTS.md, "Setup".)
# Prints PASS / WARN / FAIL per check and the exact fix for each that isn't
# PASS; exits 1 when something FAILs. Reads no secrets: of
# chimera.local.toml only whether it exists. Linux: tools/doctor.
# Usage (from the repo root): .\scripts\doctor.ps1
$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
$script:fails = 0
function Pass($what) { Write-Host "PASS  $what" }
function Warn($what, $fix) { Write-Host "WARN  $what"; Write-Host "      fix: $fix" }
function Fail($what, $fix) { Write-Host "FAIL  $what"; Write-Host "      fix: $fix"; $script:fails++ }

try {
    # Git: identity, hooks, line endings.
    $name = git config --local user.name
    $email = git config --local user.email
    if ($name -eq 'r0pi0x' -and $email -eq '339403489+r0pi0x@users.noreply.github.com') {
        Pass 'git identity (repo-local)'
    } else {
        Fail "git identity is '$name <$email>' (the pre-push hook refuses it)" '.\scripts\setup.ps1'
    }
    if ((git config core.hooksPath) -eq '.githooks') { Pass 'hooks (core.hooksPath .githooks)' }
    else { Fail 'hooks not enabled' '.\scripts\setup.ps1   (or: git config core.hooksPath .githooks)' }
    $crlf = @(git ls-files --eol | Where-Object { ($_ -split '\s+')[1] -match 'crlf' } | Select-Object -First 3)
    if ($crlf.Count -eq 0) { Pass 'line endings (LF in the working tree)' }
    else {
        Fail "CRLF files in the working tree, e.g. $(($crlf | ForEach-Object { ($_ -split '\s+')[-1] }) -join ' ')" `
            'git config core.autocrlf false; git rm -r --cached -q .; git reset --hard   (commit or stash first)'
    }
    if ($env:TZ -eq 'UTC') { Pass 'TZ=UTC in this shell' }
    else { Warn 'TZ is not UTC in this shell (commit dates must be UTC)' '$env:TZ = "UTC"   before git commit' }

    # Toolchain.
    $need = (Select-String -Path Cargo.toml -Pattern '^rust-version = "(.*)"' | Select-Object -First 1).Matches[0].Groups[1].Value
    $rustc = Get-Command rustc -ErrorAction SilentlyContinue
    if ($rustc) {
        $have = ((rustc --version) -split ' ')[1]
        if ([version]($have -replace '-.*', '') -ge [version]$need) { Pass "rustc $have (needs $need)" }
        else { Fail "rustc $have is older than $need" 'rustup update stable' }
    } else { Fail 'no rustc' 'install Rust from https://rustup.rs' }
    cargo nextest --version *> $null
    if ($LASTEXITCODE -eq 0) { Pass 'cargo-nextest' }
    else { Warn 'no cargo-nextest (optional; the full suite runs 2x faster with it)' 'cargo install cargo-nextest --locked' }
    cargo hakari --version *> $null
    if ($LASTEXITCODE -eq 0) { Pass 'cargo-hakari' }
    else { Warn 'no cargo-hakari (needed only after changing a dependency)' 'cargo install cargo-hakari --locked' }
    if (Get-Command sccache -ErrorAction SilentlyContinue) { Pass 'sccache' }
    else { Warn 'no sccache (optional: a shared compile cache)' 'cargo install sccache --locked' }

    # Local config and game installs (only existence is checked).
    if ((Test-Path chimera.local.toml) -or (Test-Path mashup.local.toml)) { Pass 'chimera.local.toml present (contents not read)' }
    else {
        Warn 'no chimera.local.toml (optional: install paths, Combat Arms keys; most tests do not need it)' `
            'Copy-Item chimera.local.example.toml chimera.local.toml and edit it yourself'
    }
    $steam = $env:CHIMERA_STEAM_ROOT
    if (-not $steam) { $steam = (Get-ItemProperty -Path 'HKCU:\Software\Valve\Steam' -Name SteamPath -ErrorAction SilentlyContinue).SteamPath }
    if (-not $steam) { $steam = "${env:ProgramFiles(x86)}\Steam" }
    $css = $null
    $vdf = Join-Path $steam 'steamapps\libraryfolders.vdf'
    if (Test-Path $vdf) {
        $libs = @($steam) + @(Select-String -Path $vdf -Pattern '^\s*"path"\s*"(.*)"' | ForEach-Object { $_.Matches[0].Groups[1].Value -replace '\\\\', '\' })
        foreach ($lib in $libs) {
            if (Test-Path (Join-Path $lib 'steamapps\appmanifest_240.acf')) { $css = $lib; break }
        }
    }
    $settings = if ($env:CHIMERA_SETTINGS) { $env:CHIMERA_SETTINGS } else { Join-Path $env:APPDATA 'chimera\settings.toml' }
    if ($css) { Pass "Counter-Strike: Source found in the Steam library $css" }
    elseif (Test-Path $settings) { Pass "Counter-Strike: Source: a saved install path may be in $settings" }
    else {
        Warn 'Counter-Strike: Source (Steam app 240) not found (tests that need it skip; the apps need it)' `
            'install it with Steam, or set [games.cs_source] path in chimera.local.toml, or run chimera_install <folder> in the game console'
    }
    if ($env:CHIMERA_ASSET_URL) { Pass 'asset service configured (CHIMERA_ASSET_URL)' }

    # Ports the game and tools use by default.
    foreach ($p in @(@('UDP', 27015), @('UDP', 28930), @('TCP', 15702))) {
        $used = if ($p[0] -eq 'UDP') { Get-NetUDPEndpoint -LocalPort $p[1] -ErrorAction SilentlyContinue }
                else { Get-NetTCPConnection -LocalPort $p[1] -State Listen -ErrorAction SilentlyContinue }
        if ($used) {
            Warn "$($p[0]) port $($p[1]) is in use by pid $(($used | Select-Object -First 1).OwningProcess)" `
                'tool runs pick free ports by themselves; else pass -port <n> / $env:CHIMERA_REMOTE_PORT = <n>'
        } else { Pass "$($p[0]) port $($p[1]) free" }
    }
} finally {
    Pop-Location
}
if ($script:fails -gt 0) { Write-Host "doctor: $($script:fails) check(s) failed."; exit 1 }
Write-Host 'doctor: ready.'
