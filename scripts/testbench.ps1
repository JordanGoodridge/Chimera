# Benchmark the test cycle (docs/plans/active/test-speed.md).
#
# Usage (from the repo root):
#   .\scripts\testbench.ps1 -Scenario fast-cargo
#   .\scripts\testbench.ps1 -Scenario leaf -Crate chimera_sim -File chimera/sim/src/lib.rs
#   .\scripts\testbench.ps1 -Scenario crate -Crate cs_source
#   .\scripts\testbench.ps1 -Scenario commit          # the pre-commit hook (bash)
#   .\scripts\testbench.ps1 -Scenario affected -Base main
#   .\scripts\testbench.ps1 -Scenario full-nextest -Repetitions 1
#   .\scripts\testbench.ps1 -Scenario heavy -Threads 6
#   .\scripts\testbench.ps1 -Scenario all -Update     # cheap scenarios, write the baseline
#
# What it records: wall, user+system CPU and peak committed memory for the
# whole process tree (a Windows job object; no sampling), the test counts
# of cargo test / nextest, and the top tests by time from nextest's JUnit
# output when nextest is installed. Raw logs and JSON go to
# target\testbench\ (git-ignored). The baseline is
# tools\testbench\baseline.json, keyed by OS/threads/RAM (no machine name).
# The default mode compares with it: warns over 10%, fails at 2x or on a
# change in the passed test count.
#
# This is a tool, never part of the fast tier. Do not run it beside another
# agent's build: wall time on a shared box swings 2-4x. Three runs, median
# and minimum; record the load. Prefer CPU time to wall when comparing.
# Run -Update only after a warm build (or with -Warmup): a cold first run
# includes compilation and is not a test-cycle baseline.

param(
    [ValidateSet("leaf", "crate", "fast-cargo", "fast-nextest", "commit",
                 "affected", "full-cargo", "full-nextest", "heavy",
                 "map-load", "sim-setup", "all")]
    [string]$Scenario = "fast-cargo",
    [string]$Crate = "chimera_sim",
    [string]$File = "",
    [string]$Base = "main",
    [string]$Filter = "",
    [string]$Features = "dev",    [int]$Repetitions = 3,
    [int]$Threads = 0,
    [int]$TimeoutSec = 14400,
    [switch]$Warmup,
    [switch]$Update,
    [switch]$Force,
    [switch]$DryRun,
    [string]$OutDir = "target\testbench"
)

$ErrorActionPreference = "Stop"
if ($Features -eq "none") { $Features = "" }
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    if (-not (Test-Path "Cargo.toml")) {
        throw "run this from a checkout of the repository (Cargo.toml not found from $root)"
    }
    $null = New-Item -ItemType Directory -Force $OutDir
    $baselinePath = Join-Path $root "tools\testbench\baseline.json"

    # ------------------------------------------------------------------
    # Job object: exact CPU and peak memory for the process tree.
    # ------------------------------------------------------------------
    $jobSource = @'
using System;
using System.Runtime.InteropServices;

public static class TestBenchJob
{
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr CreateJobObject(IntPtr attributes, string name);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);

    [DllImport("kernel32.dll", EntryPoint = "QueryInformationJobObject", SetLastError = true)]
    private static extern bool QueryBasic(IntPtr job, int infoClass, out Basic info, uint size, out uint returned);

    [DllImport("kernel32.dll", EntryPoint = "QueryInformationJobObject", SetLastError = true)]
    private static extern bool QueryExtended(IntPtr job, int infoClass, out Extended info, uint size, out uint returned);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool CloseHandle(IntPtr handle);

    [StructLayout(LayoutKind.Sequential)]
    public struct Basic
    {
        public long UserTime;
        public long KernelTime;
        public long ThisPeriodUserTime;
        public long ThisPeriodKernelTime;
        public uint TotalPageFaults;
        public uint TotalProcesses;
        public uint ActiveProcesses;
        public uint TerminatedProcesses;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct Extended
    {
        public long PerProcessUserTimeLimit;
        public long PerJobUserTimeLimit;
        public uint LimitFlags;
        public UIntPtr MinimumWorkingSetSize;
        public UIntPtr MaximumWorkingSetSize;
        public uint ActiveProcessLimit;
        public UIntPtr Affinity;
        public uint PriorityClass;
        public uint SchedulingClass;
        public ulong ReadOperationCount;
        public ulong WriteOperationCount;
        public ulong OtherOperationCount;
        public ulong ReadTransferCount;
        public ulong WriteTransferCount;
        public ulong OtherTransferCount;
        public UIntPtr ProcessMemoryLimit;
        public UIntPtr JobMemoryLimit;
        public UIntPtr PeakProcessMemoryUsed;
        public UIntPtr PeakJobMemoryUsed;
    }

    public static IntPtr Create()
    {
        return CreateJobObject(IntPtr.Zero, null);
    }

    public static bool Assign(IntPtr job, IntPtr process)
    {
        return AssignProcessToJobObject(job, process);
    }

    // CPU seconds, peak committed bytes of the job, total processes.
    public static double[] Read(IntPtr job)
    {
        Basic basic;
        Extended extended;
        uint returned;
        bool gotBasic = QueryBasic(job, 1, out basic, (uint)Marshal.SizeOf(typeof(Basic)), out returned);
        bool gotExtended = QueryExtended(job, 9, out extended, (uint)Marshal.SizeOf(typeof(Extended)), out returned);
        double cpu = gotBasic ? (basic.UserTime + basic.KernelTime) / 1e7 : 0.0;
        double peak = gotExtended ? extended.PeakJobMemoryUsed.ToUInt64() / 1048576.0 : 0.0;
        double processes = gotBasic ? basic.TotalProcesses : 0.0;
        return new double[] { cpu, peak, processes };
    }

    public static void Close(IntPtr job)
    {
        if (job != IntPtr.Zero) CloseHandle(job);
    }
}
'@
    $jobAvailable = $true
    try { Add-Type -TypeDefinition $jobSource -ErrorAction Stop } catch { $jobAvailable = $false }

    function Quote-Arg([string]$a) {
        if ($a -match '[\s"]') { return '"' + ($a -replace '"', '\"') + '"' }
        return $a
    }

    function Get-Load {
        try {
            if ($env:OS -eq "Windows_NT") {
                $cpu = Get-CimInstance Win32_Processor | Measure-Object -Property LoadPercentage -Average
                return [math]::Round([double]$cpu.Average, 1)
            }
            $text = Get-Content /proc/loadavg -Raw
            return [math]::Round([double]($text.Split(" ")[0]), 1)
        } catch { return $null }
    }

    function Get-Machine {
        $os = if ($env:OS -eq "Windows_NT") { "windows" } else { "linux" }
        $threads = [Environment]::ProcessorCount
        $ram = 0
        try {
            if ($env:OS -eq "Windows_NT") {
                $ram = [math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB)
            } else {
                $ram = [math]::Round(((Get-Content /proc/meminfo | Select-String '^MemTotal:').Line -split '\s+')[1] / 1MB)
            }
        } catch { $ram = 0 }
        return "$os-${threads}t-${ram}g"
    }

    function Get-Bash {
        if ($env:OS -eq "Windows_NT") {
            foreach ($candidate in @(
                (Join-Path $env:ProgramFiles "Git\bin\bash.exe"),
                (Join-Path $env:LOCALAPPDATA "Programs\Git\bin\bash.exe")
            )) {
                if (Test-Path $candidate) { return $candidate }
            }
        }
        $found = Get-Command bash -ErrorAction SilentlyContinue
        if ($found) { return $found.Source }
        return $null
    }

    function Get-TestCounts([string]$text) {
        $passed = 0; $failed = 0; $ignored = 0; $skipped = 0
        # cargo test / libtest: one line per binary.
        foreach ($m in [regex]::Matches($text, 'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored')) {
            $passed += [int]$m.Groups[1].Value
            $failed += [int]$m.Groups[2].Value
            $ignored += [int]$m.Groups[3].Value
        }
        # nextest: one summary; it leaves out "0 failed" when nothing failed.
        foreach ($m in [regex]::Matches($text, 'Summary \[[^\]]*\] (\d+) tests run: (\d+) passed, (?:(\d+) failed, )?(\d+) skipped')) {
            if ([int]$m.Groups[1].Value -gt $passed + $failed + $ignored) {
                $passed = [int]$m.Groups[2].Value
                $failed = if ($m.Groups[3].Success) { [int]$m.Groups[3].Value } else { 0 }
                $skipped = [int]$m.Groups[4].Value
            }
        }
        return [pscustomobject]@{ passed = $passed; failed = $failed; ignored = $ignored; skipped = $skipped }
    }

    function Get-TopTests([string]$junit, [int]$n = 20) {
        if (-not (Test-Path $junit)) { return @() }
        try {
            [xml]$xml = Get-Content $junit -Raw
            $cases = @($xml.SelectNodes("//testcase"))
            if ($cases.Count -eq 0) { return @() }
            return $cases | ForEach-Object {
                [pscustomobject]@{
                    test = if ($_.classname) { "$($_.classname)::$($_.name)" } else { $_.name }
                    seconds = [double]$_.GetAttribute("time")
                }
            } | Sort-Object seconds -Descending | Select-Object -First $n
        } catch { return @() }
    }

    function Invoke-Measured([string[]]$Argv, [string]$Label) {
        $exe = $Argv[0]
        if (-not (Get-Command $exe -ErrorAction SilentlyContinue)) {
            throw "command not found: $exe"
        }
        $psi = New-Object System.Diagnostics.ProcessStartInfo
        $psi.FileName = (Get-Command $exe).Source
        $psi.Arguments = (($Argv | Select-Object -Skip 1) | ForEach-Object { Quote-Arg $_ }) -join " "
        $psi.WorkingDirectory = $root
        $psi.UseShellExecute = $false
        $psi.RedirectStandardOutput = $true
        $psi.RedirectStandardError = $true
        $psi.CreateNoWindow = $true

        $process = New-Object System.Diagnostics.Process
        $process.StartInfo = $psi
        $job = [IntPtr]::Zero
        if ($jobAvailable) { $job = [TestBenchJob]::Create() }

        $clock = [System.Diagnostics.Stopwatch]::StartNew()
        try {
            $null = $process.Start()
            if ($jobAvailable -and $job.ToInt64() -ne 0) {
                try {
                    $handle = if ($process.Handle -is [System.Runtime.InteropServices.SafeHandle]) {
                        $process.Handle.DangerousGetHandle()
                    } else { [IntPtr]$process.Handle }
                    $null = [TestBenchJob]::Assign($job, $handle)
                } catch { }
            }
            $outTask = $process.StandardOutput.ReadToEndAsync()
            $errTask = $process.StandardError.ReadToEndAsync()
            $exited = $process.WaitForExit([Math]::Min($TimeoutSec * 1000, [int]::MaxValue))
            if (-not $exited) {
                try { & taskkill.exe /T /F /PID $process.Id 2>$null | Out-Null } catch { }
                $null = $process.WaitForExit(10000)
            }
            $stdout = $outTask.Result
            $stderr = $errTask.Result
        } finally {
            $clock.Stop()
        }

        $cpu = 0.0; $peak = 0.0; $processes = 0.0
        if ($jobAvailable -and $job.ToInt64() -ne 0) {
            try {
                $read = [TestBenchJob]::Read($job)
                $cpu = $read[0]; $peak = $read[1]; $processes = $read[2]
            } catch { }
            [TestBenchJob]::Close($job)
        }

        $stamp = (Get-Date -Format "yyyyMMdd-HHmmss")
        $safe = ($Label -replace '[^\w.-]', '-')
        $outFile = Join-Path $OutDir "$stamp-$safe.out.log"
        $errFile = Join-Path $OutDir "$stamp-$safe.err.log"
        Set-Content -Path $outFile -Value $stdout -Encoding UTF8
        Set-Content -Path $errFile -Value $stderr -Encoding UTF8
        $log = $stdout + "`n" + $stderr
        $counts = Get-TestCounts $log

        return [pscustomobject]@{
            label     = $Label
            command   = ($Argv -join " ")
            wall_s    = [math]::Round($clock.Elapsed.TotalSeconds, 3)
            cpu_s     = [math]::Round($cpu, 3)
            peak_mb   = [math]::Round($peak, 0)
            processes = [int]$processes
            passed    = $counts.passed
            failed    = $counts.failed
            ignored   = $counts.ignored
            skipped   = $counts.skipped
            exit      = if ($exited) { $process.ExitCode } else { -1 }
            timeout   = -not $exited
            stdout    = $outFile
            stderr    = $errFile
        }
    }

    function Test-Binary {
        return "cargo"
    }

    function New-Scenario([string]$name) {
        $shared = if ($Features) { @("--features", $Features) } else { @() }
        $threadArg = if ($Threads -gt 0) {
            if ($name -like "*-nextest") { @("--test-threads", "$Threads") } else { @("--test-threads=$Threads") }
        } else { @() }
        $testThreads = if ($threadArg.Count -gt 0) { @("--") + $threadArg } else { @() }
        switch ($name) {
            "leaf" {
                $target = if ($File) { $File } else { "chimera/sim/src/lib.rs" }
                if (-not $DryRun -and -not (Test-Path $target)) { throw "no such file: $target" }
                return @{ label = "leaf"; argv = @("cargo", "test", "-p", $Crate) + $shared + @("--no-run"); touch = $target }
            }
            "crate" {
                return @{ label = "crate-$Crate"; argv = @("cargo", "test", "-p", $Crate) + $shared + $testThreads }
            }
            "fast-cargo" {
                return @{ label = "fast-cargo"; argv = @("cargo", "test") + $shared + @("--no-fail-fast", "--") + $threadArg + @("--skip", "heavy::") }
            }
            "fast-nextest" {
                return @{ label = "fast-nextest"; argv = @("cargo", "nextest", "run") + $shared + @("--no-fail-fast", "--profile", "fast") + $threadArg + @("-E", "!test(/^heavy::/)") }
            }
            "commit" {
                $bash = Get-Bash
                if (-not $bash) { throw "no bash found (Git for Windows provides one)" }
                return @{ label = "commit"; argv = @($bash, ".githooks/pre-commit") }
            }
            "affected" {
                $selected = & cargo run -q -p repo_checks --bin affected -- --base $Base --format cargo 2>$null
                if ($LASTEXITCODE -ne 0 -or -not $selected) { throw "affected selected nothing (or failed) for --base $Base" }
                $list = @($selected | Where-Object { $_ -ne "" })
                Write-Host "affected selected: $($list -join ' ')" -ForegroundColor DarkGray
                return @{ label = "affected-$Base"; argv = @("cargo", "test") + $list + $shared + @("--no-fail-fast", "--", "--skip", "heavy::") }
            }
            "full-cargo" {
                return @{ label = "full-cargo"; argv = @("cargo", "test") + $shared + @("--no-fail-fast") + $testThreads }
            }
            "full-nextest" {
                return @{ label = "full-nextest"; argv = @("cargo", "nextest", "run") + $shared + @("--no-fail-fast", "--profile", "bench") + $threadArg }
            }
            "heavy" {
                return @{ label = "heavy"; argv = @("cargo", "nextest", "run") + $shared + @("--no-fail-fast", "--profile", "bench") + $threadArg + @("-E", "test(/^heavy::/)") }
            }
            "map-load" {
                $argv = @("cargo", "test", "-p", "combat_arms") + $shared + @("--test", "it", "heavy::map_load", "--", "--ignored", "--nocapture")
                return @{ label = "map-load"; argv = $argv }
            }
            "sim-setup" {
                throw "sim-setup needs the ignored microbench (docs/plans/active/test-speed.md step 1); add it before measuring"
            }
            default { throw "unknown scenario: $name" }
        }
    }

    function Get-Median([double[]]$values) {
        $sorted = @($values | Sort-Object)
        $n = $sorted.Count
        if ($n -eq 0) { return 0.0 }
        if ($n % 2 -eq 1) { return $sorted[[int](($n - 1) / 2)] }
        return ($sorted[$n / 2 - 1] + $sorted[$n / 2]) / 2.0
    }

    function Read-Baseline {
        if (-not (Test-Path $baselinePath)) { return $null }
        try { return Get-Content $baselinePath -Raw | ConvertFrom-Json } catch { return $null }
    }

    function Write-Baseline($result, $median) {
        $machine = Get-Machine
        $all = Read-Baseline
        $doc = [ordered]@{}
        if ($all) {
            foreach ($p in $all.PSObject.Properties) { $doc[$p.Name] = $p.Value }
        }
        if (-not $doc.Contains($machine)) { $doc[$machine] = [ordered]@{} }
        $doc[$machine][$result.scenario] = [ordered]@{
            wall_s   = $median.wall_s
            cpu_s    = $median.cpu_s
            peak_mb  = $median.peak_mb
            passed   = $median.passed
            failed   = $median.failed
            load     = $result.load
            dirty    = $result.dirty
            commit   = $result.commit
            date     = $result.date
        }
        $dir = Split-Path -Parent $baselinePath
        $null = New-Item -ItemType Directory -Force $dir
        $doc | ConvertTo-Json -Depth 8 | Set-Content -Path $baselinePath -Encoding UTF8
    }

    function Compare-Baseline($result, $median) {
        $all = Read-Baseline
        if (-not $all) { Write-Host "no baseline yet; run with -Update to write one" -ForegroundColor Yellow; return 0 }
        $machine = Get-Machine
        if (-not $all.$machine -or -not $all.$machine.($result.scenario)) {
            Write-Host "no baseline for $machine / $($result.scenario); run with -Update" -ForegroundColor Yellow
            return 0
        }
        $base = $all.$machine.($result.scenario)
        if ($null -ne $base.load -and $null -ne $result.load -and
            [double]$result.load -gt ([double]$base.load * 2 + 1)) {
            Write-Host "current load $($result.load) is well above the baseline's $($base.load); treat the wall delta as noise" -ForegroundColor Yellow
        }
        $rows = @(
            @{ name = "wall_s"; unit = "s"; now = $median.wall_s; base = [double]$base.wall_s; warn = 0.10; fail = 1.00 },
            @{ name = "cpu_s"; unit = "s"; now = $median.cpu_s; base = [double]$base.cpu_s; warn = 0.10; fail = 1.00 },
            @{ name = "peak_mb"; unit = "MB"; now = $median.peak_mb; base = [double]$base.peak_mb; warn = 0.10; fail = 0.50 }
        )
        $failed = $false
        foreach ($r in $rows) {
            if ($r.base -le 0) { continue }
            $delta = ($r.now - $r.base) / $r.base
            $state = "ok"
            if ($delta -gt $r.fail) { $state = "FAIL"; $failed = $true }
            elseif ($delta -gt $r.warn) { $state = "WARN" }
            $color = if ($state -eq "FAIL") { "Red" } elseif ($state -eq "WARN") { "Yellow" } else { "Gray" }
            Write-Host ("{0,-8} {1,10:N2} -> {2,10:N2} {3} {4,7:P1}  {5}" -f $r.name, $r.base, $r.now, $r.unit, $delta, $state) -ForegroundColor $color
        }
        if ($base.passed -ne $median.passed) {
            Write-Host "test count changed: $($base.passed) -> $($median.passed)" -ForegroundColor Red
            $failed = $true
        }
        if ($failed) { return 1 }
        return 0
    }

    function Format-Markdown($result, $median) {
        return "| $($result.date) | $($result.scenario) | $($median.wall_s) | $($median.cpu_s) | $($median.peak_mb) | $($median.passed)/$($median.failed) | $($result.commit) |"
    }

    $scenarios = if ($Scenario -eq "all") { @("leaf", "crate", "fast-cargo", "fast-nextest", "affected") } else { @($Scenario) }
    $overall = 0
    foreach ($name in $scenarios) {
        if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw "cargo is not on PATH" }
        if ($name -like "*-nextest" -or $name -eq "heavy") {
            & cargo nextest --version 2>$null | Out-Null
            if ($LASTEXITCODE -ne 0) {
                if ($Scenario -eq "all") {
                    Write-Host "skipping ${name}: cargo-nextest is not installed" -ForegroundColor Yellow
                    continue
                }
                throw "cargo-nextest is not installed (install it, or use the -cargo scenario)"
            }
        }
        if ($name -eq "commit" -and -not (Get-Command bash -ErrorAction SilentlyContinue)) {
            throw "bash is not on PATH (Git for Windows provides it)"
        }

        $plan = New-Scenario $name
        if ($DryRun) {
            Write-Host "would run: $($plan.argv -join ' ')" -ForegroundColor DarkGray
            continue
        }
        $date = Get-Date -Format "yyyy-MM-dd"
        $commit = (git rev-parse --short HEAD).Trim()
        $branch = (git rev-parse --abbrev-ref HEAD).Trim()
        $dirty = [bool](git status --porcelain)
        $load = Get-Load
        Write-Host "`n== $($plan.label) ==" -ForegroundColor Cyan
        Write-Host "   $($plan.argv -join ' ')" -ForegroundColor DarkGray
        Write-Host "   load $load, $($Repetitions) run(s)$(if ($Warmup) { ', one warm-up' })" -ForegroundColor DarkGray

        $runs = @()
        if ($Warmup) {
            Write-Host "   warm-up..." -ForegroundColor DarkGray
            if ($plan.touch) { (Get-Item $plan.touch).LastWriteTime = Get-Date }
            $null = Invoke-Measured $plan.argv "$($plan.label)-warmup"
        }
        for ($i = 1; $i -le $Repetitions; $i++) {
            Write-Host "   run $i..." -NoNewline
            if ($plan.touch) { (Get-Item $plan.touch).LastWriteTime = Get-Date }
            $run = Invoke-Measured $plan.argv "$($plan.label)-r$i"
            Write-Host (" wall {0}s, cpu {1}s, peak {2}MB, {3} passed / {4} failed, exit {5}" -f $run.wall_s, $run.cpu_s, $run.peak_mb, $run.passed, $run.failed, $run.exit)
            $runs += $run
        }
        if ($runs.Count -eq 0) { continue }

        $wall = @($runs | ForEach-Object { $_.wall_s })
        $cpu = @($runs | ForEach-Object { $_.cpu_s })
        $peak = @($runs | ForEach-Object { $_.peak_mb })
        $passed = @($runs | ForEach-Object { $_.passed })
        $failed = @($runs | ForEach-Object { $_.failed })
        $median = [pscustomobject]@{
            wall_s  = [math]::Round((Get-Median $wall), 3)
            cpu_s   = [math]::Round((Get-Median $cpu), 3)
            peak_mb = [math]::Round((Get-Median $peak), 0)
            passed  = (Get-Median $passed)
            failed  = (Get-Median $failed)
        }
        $min = [pscustomobject]@{
            wall_s = ($wall | Measure-Object -Minimum).Minimum
            cpu_s = ($cpu | Measure-Object -Minimum).Minimum
        }
        $result = [pscustomobject]@{
            scenario = $plan.label
            date = $date
            commit = $commit
            branch = $branch
            dirty = $dirty
            machine = Get-Machine
            os = if ($env:OS -eq "Windows_NT") { "windows" } else { "linux" }
            threads = [Environment]::ProcessorCount
            load = $load
            command = ($plan.argv -join " ")
            repetitions = $Repetitions
            runs = $runs
            median = $median
            min = $min
            junit = $null
            top_tests = @()
        }

        # nextest's JUnit, when the profile wrote it.
        $junit = Join-Path $OutDir "junit.xml"
        if ((Test-Path $junit) -and ($plan.label -like "*-nextest" -or $plan.label -eq "heavy")) {
            $copy = Join-Path $OutDir "$date-$($plan.label)-junit.xml"
            Copy-Item $junit $copy -Force
            $result.junit = $copy
            $result.top_tests = @(Get-TopTests $copy 20)
            if ($result.top_tests.Count -gt 0) {
                Write-Host "   slowest tests:" -ForegroundColor DarkGray
                $result.top_tests | Select-Object -First 5 | ForEach-Object {
                    Write-Host ("     {0,8:N2}s  {1}" -f $_.seconds, $_.test) -ForegroundColor DarkGray
                }
            }
        }

        $json = Join-Path $OutDir "$date-$($plan.label).json"
        $result | ConvertTo-Json -Depth 8 | Set-Content -Path $json -Encoding UTF8
        Write-Host "median: wall $($median.wall_s)s (min $($min.wall_s)), cpu $($median.cpu_s)s, peak $($median.peak_mb)MB" -ForegroundColor Green
        Write-Host "json: $json" -ForegroundColor DarkGray

        $runFailed = @($runs | Where-Object { $_.exit -ne 0 -or $_.failed -gt 0 }).Count -gt 0
        if ($Update) {
            if ($runFailed -and -not $Force) {
                Write-Host "not updating the baseline: the run failed (use -Force to record it)" -ForegroundColor Red
                $overall = 1
            } else {
                Write-Baseline $result $median
                Write-Host "baseline updated: $baselinePath" -ForegroundColor Green
            }
        } else {
            $verdict = Compare-Baseline $result $median
            if ($verdict -ne 0) { $overall = $verdict }
        }
        if ($runFailed) {
            Write-Host "run had failures (exit/failed tests); not a passing benchmark" -ForegroundColor Red
            if ($overall -eq 0) { $overall = 1 }
        }
        Write-Host (Format-Markdown $result $median) -ForegroundColor DarkGray
    }

    exit $overall
} finally {
    Pop-Location
}
