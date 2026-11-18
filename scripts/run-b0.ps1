param([switch]$SkipTests)

$ErrorActionPreference = 'Stop'
Set-Location (Resolve-Path (Join-Path $PSScriptRoot '..'))

if (-not (Test-Path 'Cargo.lock')) {
    Write-Host 'Cargo.lock is missing. Resolving initial lockfile...'
    cargo generate-lockfile
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

if (-not $SkipTests) {
    cargo test --workspace --locked
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

cargo run --release --locked -p sandbox-game
exit $LASTEXITCODE
