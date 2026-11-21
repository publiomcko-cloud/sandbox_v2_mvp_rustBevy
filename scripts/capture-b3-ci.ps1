# B3 autonomous GPU streaming smoke. Zero Rust compilation on this runner.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$runtime = Join-Path (Get-Location).Path 'b3-runtime'
$report = Join-Path (Get-Location).Path 'b3-visual-report'
New-Item -ItemType Directory -Force -Path $report | Out-Null
$exe = Join-Path $runtime 'sandbox-game.exe'
if (-not (Test-Path $exe)) { throw "B3 executable is missing: $exe" }

$env:WGPU_BACKEND = 'vulkan'
$env:SANDBOX_STAGE = 'B3'
$env:SANDBOX_B3_HEADLESS = '1'
$env:SANDBOX_B3_ROUTE = 'straight'
$env:SANDBOX_B2_CAPTURE_PATH = Join-Path $report 'b3-route.png'
$env:RUST_LOG = 'info'
$env:RUST_BACKTRACE = '1'
$stdout = Join-Path $report 'stdout.log'
$stderr = Join-Path $report 'stderr.log'
Write-Host "B3 GPU smoke runner=$env:RUNNER_NAME computer=$env:COMPUTERNAME"

$p = Start-Process -FilePath $exe -WorkingDirectory $runtime -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
try {
    if (-not $p.WaitForExit(65000)) {
        Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
        throw 'B3 route timed out without screenshot'
    }
    $p.Refresh()
    if ($null -ne $p.ExitCode -and [int]$p.ExitCode -ne 0) {
        throw "B3 process exit code $($p.ExitCode)"
    }
    $png = $env:SANDBOX_B2_CAPTURE_PATH
    if (-not (Test-Path $png)) { throw 'No B3 GPU screenshot was produced' }
    if ((Get-Item $png).Length -lt 2000) { throw 'B3 screenshot is too small' }
    $logContent = Get-Content -LiteralPath $stderr -Raw
    if ($logContent -notmatch 'Screenshot saved to') {
        throw 'B3 rendered screenshot not confirmed in GPU log'
    }
    if ($logContent -match '(?m)\bERROR\b|Validation Error|panicked') {
        throw 'B3 GPU reported an error; inspect attached log'
    }
    $telemetry = @(Get-Content $stderr | Where-Object { $_ -match 'B3 horizon coverage' })
    $telemetry | Set-Content (Join-Path $report 'coverage-summary.txt')
    if ($telemetry.Count -lt 2) {
        throw 'B3 emitted insufficient 130m horizon telemetry'
    }
    Write-Host "B3 GPU screenshot produced: $png; coverage samples: $($telemetry.Count)"
} finally {
    if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
}
