# B3 long-route GPU diagnostic: reuse the successful Windows Release.
# Runs ~45 s, 5 s stationary warm-up followed by 18 m/s straight flight.
# No screenshot path is configured: the existing binary will NOT auto-exit at 7 s.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = (Get-Location).Path
$runtime = Join-Path $root 'b3-runtime'
$report = Join-Path $root 'b3-long-route-report'
New-Item -ItemType Directory -Force -Path $report | Out-Null
$exe = Join-Path $runtime 'sandbox-game.exe'
if (-not (Test-Path $exe)) { throw "Missing B3 executable: $exe" }

$env:SANDBOX_STAGE = 'B3'
$env:WGPU_BACKEND = 'vulkan'
$env:SANDBOX_B3_HEADLESS = '1'
$env:SANDBOX_B3_ROUTE = 'straight'
Remove-Item Env:\SANDBOX_B2_CAPTURE_PATH -ErrorAction SilentlyContinue
$env:RUST_LOG = 'info,sandbox_gpu_terrain::b2=warn,sandbox_gpu_terrain::b2::b3=info'
$env:RUST_BACKTRACE = '1'

$stdout = Join-Path $report 'stdout.log'
$stderr = Join-Path $report 'stderr.log'
Write-Host "B3 45-second GPU streaming route, runner=$env:RUNNER_NAME"
$p = Start-Process -FilePath $exe -WorkingDirectory $runtime -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
try {
    $finishedEarly = $p.WaitForExit(45000)
    if ($finishedEarly) {
        $p.Refresh()
        throw 'B3 game terminated early before the planned 45-second route'
    }
} finally {
    if (-not $p.HasExited) {
        Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    }
}

$raw = Get-Content -LiteralPath $stderr -Raw
$clean = [regex]::Replace($raw, '\x1b\[[0-9;]*[A-Za-z]', '')
$records = New-Object System.Collections.Generic.List[Object]
$fpsSamples = New-Object System.Collections.Generic.List[Double]
$frameSamples = New-Object System.Collections.Generic.List[Double]
foreach ($line in ($clean -split "[\r\n]+")) {
    if ($line -match 'B3 horizon coverage' -and
        $line -match 'required=(\d+).*visible=(\d+).*missing=(\d+).*resident=(\d+).*evictions=(\d+).*x=(-?[0-9.]+)') {
        $records.Add([pscustomobject]@{
            required = [int]$Matches[1]
            visible = [int]$Matches[2]
            missing = [int]$Matches[3]
            resident = [int]$Matches[4]
            evictions = [int]$Matches[5]
            x = [double]$Matches[6]
        })
    }
    if ($line -match '\bfps\s+:\s+([0-9.]+)') { $fpsSamples.Add([double]$Matches[1]) }
    if ($line -match '\bframe_time\s+:\s+([0-9.]+)ms') { $frameSamples.Add([double]$Matches[1]) }
}

$summary = @(
    "B3 long-route diagnostic; compiled binary from Actions run 44895882768",
    "Runner: $env:RUNNER_NAME",
    "Sampling duration: 45 seconds (5 seconds initial warmup)",
    "Horizon coverage samples: $($records.Count)",
    "FPS samples: $($fpsSamples.Count)",
    "Frame-time samples: $($frameSamples.Count)"
)
if ($records.Count -gt 0) {
    $last = $records[$records.Count-1]
    $first = $records[0]
    $failures = @($records | Where-Object { $_.missing -ne 0 })
    $distance = $last.x - $first.x
    $summary += "Movement X: $($first.x) to $($last.x) m, delta=$distance m"
    $summary += "Final required=$($last.required) visible=$($last.visible) resident=$($last.resident) evictions=$($last.evictions)"
    $summary += "Coverage samples with missing blocks: $($failures.Count)"
    if ($fpsSamples.Count -gt 0) {
        $m = $fpsSamples | Measure-Object -Minimum -Maximum -Average
        $summary += "Sampled FPS min=$($m.Minimum) avg=$($m.Average) max=$($m.Maximum)"
    }
    if ($frameSamples.Count -gt 0) {
        $m = $frameSamples | Measure-Object -Minimum -Maximum -Average
        $summary += "Sampled frame time (ms) min=$($m.Minimum) avg=$($m.Average) max=$($m.Maximum)"
    }
}

$summary | Set-Content -LiteralPath (Join-Path $report 'summary.txt') -Encoding UTF8
$records | Export-Csv -NoTypeInformation -Path (Join-Path $report 'coverage.csv')
$summary | ForEach-Object { Write-Host $_ }

if ($clean -match '(?m)\bERROR\b|Validation Error|panicked') {
    throw 'GPU runtime logged an error; inspect stderr.log'
}
if ($records.Count -lt 25) { throw 'Too few long-route coverage samples' }
if (($last.x - $first.x) -lt 450.0) {
    throw 'Route did not cover at least 450m at the configured 18m/s'
}
if ($failures.Count -gt 0) {
    throw 'B3 fails zero-missing-block invariant on long route'
}
# FPS is reported but NOT passed solely from one-second samples.
# Mixed-LOD and real present-to-present timings remain future gates.
