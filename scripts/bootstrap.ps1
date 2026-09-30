$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
  throw 'Node.js 22 or newer is required.'
}

$nodeMajor = [int]((node -v).TrimStart('v').Split('.')[0])
if ($nodeMajor -lt 22) {
  throw "Node.js 22 or newer is required. Found $(node -v)."
}

corepack enable
corepack prepare pnpm@10.15.1 --activate
pnpm install
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
pnpm lint
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
pnpm typecheck
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo test --manifest-path native/Cargo.toml
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
pnpm test
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
pnpm build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
pnpm smoke
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Output 'JARVIG_OK bootstrap'
