param([string]$Bun = "bun")
$ErrorActionPreference = "Stop"
$repo = Split-Path $PSScriptRoot -Parent
$ui = Join-Path $repo "assets/ui/pocket"
$compiler = Join-Path $ui "node_modules/@pocketjs/framework/microts/compiler/cli.ts"
if (!(Test-Path $compiler)) { throw "Run npm ci --ignore-scripts in assets/ui/pocket first." }
# npm can skip Bun's postinstall; its optional Windows package contains the real executable.
if ($Bun -eq "bun" -and !(Get-Command bun -ErrorAction SilentlyContinue)) {
    $Bun = Join-Path $ui "node_modules/@oven/bun-windows-x64/bin/bun.exe"
}
& $Bun $compiler build (Join-Path $ui "Desktop.vue") --strict
if ($LASTEXITCODE -ne 0) { throw "MicroTS compilation failed." }
