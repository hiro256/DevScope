[CmdletBinding()]
param(
    [switch]$Rust
)

$ErrorActionPreference = 'Stop'

if ($Rust) {
    cargo check
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

git diff --check
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

git diff --ignore-space-at-eol
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

git status --short
