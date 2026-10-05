param([Parameter(Mandatory=$true)][string]$HostRepo, [string]$CargoWrapper)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path -LiteralPath $HostRepo).Path
$tests = Join-Path $repo 'crates/mod-host/tests'
New-Item -ItemType Directory -Force -Path $tests | Out-Null
$fixture = Join-Path $tests 'cinnaroids_callback_budget.rs'
if (Test-Path -LiteralPath $fixture) { throw "Test fixture already exists: $fixture" }
$previous = $env:CINNAROIDS_BUDGET_COMPONENT
$env:CINNAROIDS_BUDGET_COMPONENT = (Resolve-Path (Join-Path $PSScriptRoot '../assets/cinnaroids.component.wasm')).Path
try {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'host-budget.rs') -Destination $fixture
    Push-Location -LiteralPath $repo
    try {
        $arguments = @('test', '--locked', '-p', 'mod-host', '--test', 'cinnaroids_callback_budget')
        if ($CargoWrapper) { & $CargoWrapper -CargoArgs $arguments } else { & cargo @arguments }
        if ($LASTEXITCODE -ne 0) { throw 'Real WASM callback budget regression failed.' }
    } finally { Pop-Location }
} finally {
    Remove-Item -LiteralPath $fixture
    $env:CINNAROIDS_BUDGET_COMPONENT = $previous
}
