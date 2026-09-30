$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
if (-not (Test-Path 'node_modules')) {
  throw 'Dependencies are not installed. Run scripts/bootstrap.ps1 first.'
}
pnpm dev:client
exit $LASTEXITCODE
