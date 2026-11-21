# B3 mixed LOD 0/1/2 hardware gate. Does not promote the candidate.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = (Get-Location).Path
$runtime = Join-Path $root 'b3-runtime'
$report = Join-Path $root 'b3-mixed-report'
New-Item -ItemType Directory -Force -Path $report | Out-Null
$exe = Join-Path $runtime 'sandbox-game.exe'
if (-not (Test-Path -LiteralPath $exe)) { throw "Missing Release executable: $exe" }
$env:SANDBOX_STAGE = 'B3'
$env:SANDBOX_B3_HEADLESS = '1'
$env:SANDBOX_B3_ROUTE = 'straight'
$env:SANDBOX_B3_RENDER_1080P = '1'
$env:SANDBOX_B3_MIXED_LOD = '1'
$env:SANDBOX_B3_CAPTURE_AFTER_SECONDS = '45'
$env:WGPU_BACKEND = 'vulkan'
$env:RUST_LOG = 'info,sandbox_gpu_terrain::b2=warn,sandbox_gpu_terrain::b2::b3=info'
$env:SANDBOX_B2_CAPTURE_PATH = Join-Path $report 'mixed.png'
$env:RUST_BACKTRACE = '1'
$stdout = Join-Path $report 'stdout.log'
$stderr = Join-Path $report 'stderr.log'
Write-Host "B3 mixed hardware test runner=$env:RUNNER_NAME"
$p = Start-Process -FilePath $exe -WorkingDirectory $runtime -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
try {
    if (-not $p.WaitForExit(80000)) {
        Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
        throw "B3 mixed route timed out"
    }
    $p.Refresh()
    if ($null -ne $p.ExitCode -and $p.ExitCode -ne 0) { throw "B3 mixed process exited $($p.ExitCode)" }
    if (-not (Test-Path -LiteralPath $env:SANDBOX_B2_CAPTURE_PATH)) { throw "No captured GPU frame" }
    if ((Get-Item -LiteralPath $env:SANDBOX_B2_CAPTURE_PATH).Length -lt 2000) { throw "Invalid GPU frame" }
    $log = (Get-Content -LiteralPath $stderr -Raw) -replace '\x1b\[[0-9;]*m', ''
    if ($log -match '(?m)\bERROR\b|Validation Error|panicked') { throw "GPU or runtime error; inspect stderr artifact" }
    if ($log -notmatch 'Screenshot saved to') { throw "No GPU screenshot confirmation" }
    if ($log -notmatch 'GTX 1050 Ti') { throw "Expected GTX 1050 Ti GPU; device was not verified" }
    $coverage = @($log -split "[\r\n]+" | Where-Object { $_ -match 'B3 horizon coverage' })
    if ($coverage.Count -lt 20) { throw "Too few B3 horizon samples: $($coverage.Count)" }
    $rows = @()
    foreach ($line in $coverage) {
        $keys = @{}
        foreach ($name in @('required','visible','missing','x','lod0','lod1','lod2','seam_blocks','transition_pending','transition_queued','transition_commits')) {
            if ($line -match ("\b" + $name + "=(-?[0-9.]+)")) { $keys[$name] = $Matches[1] }
        }
        foreach ($mandatory in @('required','visible','missing','x','lod0','lod1','lod2','seam_blocks','transition_commits')) {
            if (-not $keys.ContainsKey($mandatory)) { throw "Missing $mandatory in B3 telemetry: $line" }
        }
        $rows += [pscustomobject]@{
            x = [double]$keys.x
            required = [int]$keys.required
            visible = [int]$keys.visible
            missing = [int]$keys.missing
            lod0 = [int]$keys.lod0
            lod1 = [int]$keys.lod1
            lod2 = [int]$keys.lod2
            seams = [int]$keys.seam_blocks
            commits = [int]$keys.transition_commits
        }
    }
    $rows | Export-Csv -NoTypeInformation -Encoding UTF8 -Path (Join-Path $report 'coverage.csv')
    $origin = $rows[0].x
    $moving = @($rows | Where-Object { [Math]::Abs($_.x - $origin) -gt 0.25 })
    $mixed = @($rows | Where-Object { $_.lod0 -gt 0 -and $_.lod1 -gt 0 -and $_.lod2 -gt 0 -and $_.seams -gt 0 })
    $lost = @($moving | Where-Object { $_.missing -gt 0 })
    $distance = $rows[-1].x - $origin
    $maxCommits = ($rows | Measure-Object -Property commits -Maximum).Maximum
    @(
        "runner=$env:RUNNER_NAME"
        "device=GTX 1050 Ti"
        "coverage_samples=$($rows.Count)"
        "moving_samples=$($moving.Count)"
        "missing_during_motion=$($lost.Count)"
        "mixed_lod_samples=$($mixed.Count)"
        "transition_commits=$maxCommits"
        "distance_m=$distance"
        "headless_capture_only=true"
        "full_present_frame_budget_qualified=false"
    ) | Set-Content -Path (Join-Path $report 'summary.txt')
    Get-Content (Join-Path $report 'summary.txt') | ForEach-Object { Write-Host $_ }
    if ($moving.Count -lt 20 -or $distance -lt 500) { throw "Insufficient continuous 18 m/s movement" }
    if ($lost.Count -gt 0) { throw "Missing required terrain while moving" }
    if ($mixed.Count -lt 3 -or $maxCommits -lt 1) { throw "Three-tier mixed LOD with GPU seams never became visible" }
} finally {
    if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
}
