# Controlled A/B comparison for B3, using the same compiled Windows Release.
# 1920x1080 headless Vulkan, uncapped scheduler, 18m/s, 45s route (5s warmup).
# The benchmark should not be interpreted as a 1920x1080 interactive present test.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = (Get-Location).Path
$runtime = Join-Path $root 'b3-runtime'
$report = Join-Path $root 'b3-ab-report'
New-Item -ItemType Directory -Force -Path $report | Out-Null
$exe = Join-Path $runtime 'sandbox-game.exe'
if (-not (Test-Path $exe)) { throw "No prebuilt release at $exe" }

$env:SANDBOX_STAGE = 'B3'
$env:SANDBOX_B3_ROUTE = 'straight'
$env:SANDBOX_B3_HEADLESS = '1'
$env:SANDBOX_B3_RENDER_1080P = '1'
$env:SANDBOX_B3_BENCH_UNCAPPED = '1'
$env:SANDBOX_B3_CAPTURE_AFTER_SECONDS = '45'
$env:WGPU_BACKEND = 'vulkan'
$env:RUST_BACKTRACE = '1'
$env:RUST_LOG = 'info,sandbox_gpu_terrain::b2=warn,sandbox_gpu_terrain::b2::b3=info'

$results = @()
foreach ($arm in @('baseline','optimized')) {
    $env:SANDBOX_B3_SCHEDULER = $arm
    $env:SANDBOX_B2_CAPTURE_PATH = Join-Path $report "$arm.png"
    $stdout = Join-Path $report "$arm.stdout.log"
    $stderr = Join-Path $report "$arm.stderr.log"
    Write-Host "B3 A/B: $arm @ 1920x1080, no release build"

    $p = Start-Process -FilePath $exe -WorkingDirectory $runtime -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    try {
        if (-not $p.WaitForExit(75000)) {
            Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
            throw "B3 $arm timed out without end-of-route PNG"
        }
        $p.Refresh()
        # Windows PowerShell 5.1 may expose null ExitCode for Start-Process.
        if ($null -ne $p.ExitCode -and [int]$p.ExitCode -ne 0) {
            throw "B3 $arm exited with $($p.ExitCode)"
        }
    } finally {
        if (-not $p.HasExited) {
            Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
        }
    }

    $png = $env:SANDBOX_B2_CAPTURE_PATH
    if (-not (Test-Path $png) -or (Get-Item $png).Length -lt 2000) {
        throw "B3 $arm did not produce a valid PNG file"
    }
    $log = Get-Content -LiteralPath $stderr -Raw
    if ($log -notmatch 'Screenshot saved to') {
        throw "B3 $arm did not confirm GPU frame capture"
    }
    if ($log -match '(?m)\bERROR\b|Validation Error|panicked') {
        throw "B3 $arm logged a render error"
    }

    $fps = New-Object System.Collections.Generic.List[Double]
    $frameMs = New-Object System.Collections.Generic.List[Double]
    $streamCpuMax = New-Object System.Collections.Generic.List[Double]
    $missing = New-Object System.Collections.Generic.List[Int32]
    $distance = New-Object System.Collections.Generic.List[Double]
    foreach ($line in ($log -split "[\r\n]+")) {
        if ($line -match '\bfps\s+:\s+([0-9.]+)') { $fps.Add([double]$Matches[1]) }
        if ($line -match '\bframe_time\s+:\s+([0-9.]+)ms') { $frameMs.Add([double]$Matches[1]) }
        if ($line -match 'B3 horizon coverage') {
            if ($line -match 'required=(\d+).*visible=(\d+).*missing=(\d+)') {
                $missing.Add([int]$Matches[3])
            }
            if ($line -match '\bcpu_max_us=(\d+)') { $streamCpuMax.Add([double]$Matches[1]) }
            if ($line -match '\bx=(-?[0-9.]+)') { $distance.Add([double]$Matches[1]) }
        }
    }
    if ($fps.Count -lt 20 -or $frameMs.Count -lt 20 -or $missing.Count -lt 20) {
        throw "B3 $arm diagnostics incomplete: fps=$($fps.Count), frames=$($frameMs.Count), coverage=$($missing.Count)"
    }

    $framesSorted = @($frameMs | Sort-Object)
    $fpsSorted = @($fps | Sort-Object)
    $p95Frame = $framesSorted[[Math]::Max(0, [int][Math]::Ceiling($framesSorted.Count * 0.95) - 1)]
    $p99Frame = $framesSorted[[Math]::Max(0, [int][Math]::Ceiling($framesSorted.Count * 0.99) - 1)]
    $cpuMaxMs = if ($streamCpuMax.Count) {
        ($streamCpuMax | Measure-Object -Maximum).Maximum / 1000.0
    } else { -1.0 }
    $missingCount = @($missing | Where-Object { $_ -gt 0 }).Count
    $deltaX = $distance[$distance.Count-1] - $distance[0]
    $m = $frameMs | Measure-Object -Minimum -Maximum -Average
    $f = $fps | Measure-Object -Minimum -Maximum -Average

    $result = [pscustomobject]@{
        arm=$arm
        samples=$frameMs.Count
        distance_m=[Math]::Round($deltaX,2)
        coverage_missing_samples=$missingCount
        fps_min=[Math]::Round($f.Minimum,3)
        fps_average=[Math]::Round($f.Average,3)
        fps_max=[Math]::Round($f.Maximum,3)
        frame_ms_average=[Math]::Round($m.Average,3)
        frame_ms_p95=[Math]::Round($p95Frame,3)
        frame_ms_p99=[Math]::Round($p99Frame,3)
        frame_ms_max=[Math]::Round($m.Maximum,3)
        stream_cpu_us_max=[Math]::Round($cpuMaxMs * 1000,0)
    }
    $results += $result
    $result | Format-List | Out-String | Write-Host
}

$results | Export-Csv -Path (Join-Path $report 'compare.csv') -NoTypeInformation -Encoding UTF8
$results | ConvertTo-Json -Depth 3 | Set-Content -Path (Join-Path $report 'compare.json') -Encoding UTF8

$baseline = $results[0]
$optimized = $results[1]
@(
    "Experiment: frame-budget B3 1920x1080 uncapped Vulkan A/B, 18m/s, two 45-second routes",
    "Comparison is intra-build; both arms use identical Bevy GPU rendering, shader, and logging.",
    "Baseline FPS avg=$($baseline.fps_average), optimized avg=$($optimized.fps_average)",
    "Baseline p99 frame(ms)=$($baseline.frame_ms_p99), optimized p99=$($optimized.frame_ms_p99)",
    "Baseline max frame(ms)=$($baseline.frame_ms_max), optimized max=$($optimized.frame_ms_max)",
    "Baseline CPU update max(us)=$($baseline.stream_cpu_us_max), optimized max(us)=$($optimized.stream_cpu_us_max)",
    "Baseline/optimized missing samples=$($baseline.coverage_missing_samples)/$($optimized.coverage_missing_samples)",
    "Strict 16.7ms per-frame gate: optimized measured max <=16.7? $($optimized.frame_ms_max -le 16.7)",
    "LIMITATION: headless GPU frames are NOT an interactive Windows present/flip benchmark.",
    "LIMITATION: the three-tier LOD frontier is not yet a welded render path; B3 remains HOLD."
) | Set-Content -Path (Join-Path $report 'summary.txt') -Encoding UTF8

if ($baseline.coverage_missing_samples -gt 0 -or $optimized.coverage_missing_samples -gt 0) {
    throw 'B3 130m coverage gap detected in one or both A/B arms'
}
if ($baseline.distance_m -lt 500 -or $optimized.distance_m -lt 500) {
    throw 'B3 A/B route distance shorter than 500m'
}
