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
            if ($process.ExitCode -ne 0) {
                $problems += "$scene : game exited with code $($process.ExitCode)"
            }
        }
        $picture = $env:SANDBOX_B2_CAPTURE_PATH
        if ((Test-Path $picture) -and ((Get-Item $picture).Length -gt 2000)) {
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
