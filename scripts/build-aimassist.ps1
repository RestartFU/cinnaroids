param([string]$CargoWrapper = $env:CINNABAR_CARGO_WRAPPER)
$ErrorActionPreference = 'Stop'
$repoPath = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$guestPath = Join-Path $repoPath 'mods/aimassist'
$targetPath = Join-Path $guestPath 'target'
$outputPath = Join-Path $repoPath 'assets/aimassist.component.wasm'

# Codex builds share a bounded Cargo slot; ordinary checkouts can build directly.
if (-not $CargoWrapper) {
    $sharedWrapper = Join-Path $PSScriptRoot '../../cargo-slot.ps1'
    if (Test-Path -LiteralPath $sharedWrapper) {
        $CargoWrapper = (Resolve-Path -LiteralPath $sharedWrapper).Path
    }
}
function Invoke-AimCargo([string[]]$Arguments) {
    if ($CargoWrapper) {
        & $CargoWrapper -CargoArgs $Arguments
    } else {
        $env:CARGO_TARGET_DIR = $targetPath
        $env:CARGO_BUILD_JOBS = '4'
        & cargo @Arguments
    }
    if ($LASTEXITCODE -ne 0) { throw "Cargo failed with exit code $LASTEXITCODE" }
}

Push-Location -LiteralPath $guestPath
try {
    Invoke-AimCargo @('build', '--locked', '--release', '-p', 'cinnabar-aimassist-mod', '--target', 'wasm32-unknown-unknown')
    $corePath = Join-Path $targetPath 'wasm32-unknown-unknown/release/cinnabar_aimassist_mod.wasm'
    Invoke-AimCargo @('run', '--locked', '-p', 'cinnabar-aimassist-pack', '--', $corePath, $outputPath)
} finally {
    Pop-Location
}
Write-Output "Built $outputPath"
