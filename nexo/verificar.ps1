# Verificación completa antes de entregar: calidad, pruebas, compilación y simulación.
# Uso (desde la carpeta nexo):  powershell -ExecutionPolicy Bypass -File .\verificar.ps1
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

Write-Host "1/4  Analizador estático (cargo clippy)..." -ForegroundColor Cyan
cargo clippy --release -- -D warnings
if ($LASTEXITCODE -ne 0) { throw "clippy encontró advertencias" }

Write-Host "2/4  Pruebas automatizadas (cargo test)..." -ForegroundColor Cyan
cargo test --release
if ($LASTEXITCODE -ne 0) { throw "fallaron pruebas" }

Write-Host "3/4  Ejecutable final (cargo build --release)..." -ForegroundColor Cyan
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "falló la compilación" }

Write-Host "4/4  Simulación del modo automático (no borra nada)..." -ForegroundColor Cyan
$sim = Join-Path $env:TEMP "nexo_simulacion.txt"
Start-Process ".\target\release\nexo.exe" -ArgumentList "--simular-auto", $sim -Wait
Get-Content $sim -Encoding UTF8 | Select-Object -Last 1
if (-not (Select-String -Path $sim -Pattern "videos/documentos incluidos: 0" -Quiet)) { throw "el automático incluiría videos o documentos" }

$exe = Get-Item ".\target\release\nexo.exe"
Write-Host ("Listo: {0} ({1:N1} MB)" -f $exe.FullName, ($exe.Length / 1MB)) -ForegroundColor Green
