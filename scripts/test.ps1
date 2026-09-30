$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
cargo test --manifest-path native/Cargo.toml
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
pnpm lint
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
pnpm typecheck
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
pnpm test
exit $LASTEXITCODE
