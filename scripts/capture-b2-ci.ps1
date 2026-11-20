# B2 visual verification on a qualified Vulkan Actions runner.
# Captures the application's render target, not the Windows desktop.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = (Get-Location).Path
$runtime = Join-Path $root 'b2-runtime'
$report = Join-Path $root 'b2-visual-report'
New-Item -ItemType Directory -Force -Path $report | Out-Null
$exe = Join-Path $runtime 'sandbox-game.exe'
if (-not (Test-Path $exe)) { throw "Missing release: $exe" }
foreach ($shader in @('sdf_density.wgsl', 'surface_nets.wgsl')) {
    $file = Join-Path $runtime "assets/shaders/$shader"
    if (-not (Test-Path $file)) { throw "Missing shader: $file" }
}

$env:WGPU_BACKEND = 'vulkan'
$env:SANDBOX_B2_LOD = '1'
$env:SANDBOX_B2_HEADLESS = '1'
$env:RUST_BACKTRACE = '1'
$env:RUST_LOG = 'info'
$problems = @()

foreach ($scene in @('surface', 'cave')) {
    $env:SANDBOX_B2_SCENE = $scene
    $env:SANDBOX_B2_CAPTURE_PATH = Join-Path $report "$scene.png"
    $stdout = Join-Path $report "$scene.stdout.log"
    $stderr = Join-Path $report "$scene.stderr.log"
    Write-Host "B2 visual smoke: $scene on $env:COMPUTERNAME"
    $process = Start-Process -FilePath $exe -WorkingDirectory $runtime -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    try {
        if (-not $process.WaitForExit(65000)) {
            $problems += "$scene : application did not exit within 65 seconds"
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        } else {
            $process.Refresh()
            # On Windows PowerShell 5.1, Start-Process -PassThru
            # may expose $null for ExitCode after process termination.
            # Null is UNKNOWN, never a nonzero failure.
            $code = $null
            try { $code = $process.ExitCode } catch { }
            if ($null -ne $code) {
                Write-Host "B2 visual exit code for $scene : $code"
                if ([int]$code -ne 0) {
                    $problems += "$scene : game exited with code $code"
                }
            } else {
                Write-Host "B2 visual: PowerShell returned no exit code for $scene; verifying PNG and GPU logs"
            }
        }
        $picture = $env:SANDBOX_B2_CAPTURE_PATH
        if ((Test-Path $picture) -and ((Get-Item $picture).Length -gt 2000)) {
            try {
                Add-Type -AssemblyName System.Drawing
                $bitmap = [System.Drawing.Bitmap]::FromFile($picture)
                try {
                    if ($bitmap.Width -ne 1280 -or $bitmap.Height -ne 720) {
                        $problems += "$scene : unexpected PNG size $($bitmap.Width)x$($bitmap.Height)"
                    }
                } finally { $bitmap.Dispose() }
            } catch {
                $problems += "$scene : invalid PNG: $_"
            }
            $logContent = Get-Content -LiteralPath $stderr -Raw
            if ($logContent -notmatch 'Screenshot saved to' -or $logContent -notmatch 'B2 GPU density/extraction dispatched') {
                $problems += "$scene : PNG exists but GPU render completion logs are missing"
            }
            if ($logContent -match '(?m)\bERROR\b|Validation Error|panic') {
                $problems += "$scene : GPU runtime log contains an error"
            }
            Write-Host "B2 visual PNG captured: $picture"
        } else {
            $problems += "$scene : Bevy GPU frame screenshot unavailable"
        }
    } finally {
        if (-not $process.HasExited) {
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        }
    }
}

$reportText = @(
    "Runner: $env:RUNNER_NAME",
    "Computer: $env:COMPUTERNAME",
    "Windows session: $([System.Diagnostics.Process]::GetCurrentProcess().SessionId)",
    "B2 world: seed 42, 8 blocks, LOD1",
    "Visual smoke problems: $($problems.Count)"
) + $problems
$reportText | Set-Content -Path (Join-Path $report 'status.txt') -Encoding UTF8
$reportText | ForEach-Object { Write-Host $_ }

if ($problems.Count -gt 0) {
    throw 'B2 visual smoke unavailable. Inspect screenshot/log artifacts; Windows service graphics may be restricted.'
}
