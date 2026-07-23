# Script to run test_withdraw_bid_matrix tests
# This script handles the complex Windows build environment setup

param(
    [switch]$UseWSL,
    [switch]$UseDocker,
    [switch]$InstallDeps
)

function Check-Rust {
    try {
        $cargoVersion = & cargo --version
        Write-Host "✓ Cargo is available: $cargoVersion" -ForegroundColor Green
        return $true
    } catch {
        Write-Host "✗ Cargo not found. Rust toolchain may need to be installed or PATH needs to be refreshed." -ForegroundColor Red
        return $false
    }
}

function Check-BuildTools {
    $linkExePath = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC"
    if (Test-Path $linkExePath) {
        Write-Host "✓ Visual Studio Build Tools detected" -ForegroundColor Green
        return $true
    } else {
        Write-Host "✗ Visual Studio Build Tools not found" -ForegroundColor Yellow
        return $false
    }
}

function Run-WithDocker {
    Write-Host "`nAttempting to run tests with Docker..." -ForegroundColor Cyan
    try {
        & docker run --rm -v "${PWD}:/workspace" -w /workspace/quicklendx-contracts rust:latest cargo test -p quicklendx-contracts test_withdraw_bid_matrix -- --nocapture
        if ($LASTEXITCODE -eq 0) {
            Write-Host "✓ Tests completed successfully via Docker!" -ForegroundColor Green
            return $true
        }
    } catch {
        Write-Host "✗ Docker execution failed: $_" -ForegroundColor Red
    }
    return $false
}

function Run-WithWSL {
    Write-Host "`nAttempting to run tests with WSL..." -ForegroundColor Cyan
    try {
        $projectPath = (Get-Location).Path -replace '\\', '/'
        $projectPath = $projectPath -replace 'C:/', '/mnt/c/'
        & wsl bash -c "cd '$projectPath/quicklendx-contracts' && cargo test -p quicklendx-contracts test_withdraw_bid_matrix -- --nocapture"
        if ($LASTEXITCODE -eq 0) {
            Write-Host "✓ Tests completed successfully via WSL!" -ForegroundColor Green
            return $true
        }
    } catch {
        Write-Host "✗ WSL execution failed: $_" -ForegroundColor Red
    }
    return $false
}

function Run-Native {
    Write-Host "`nRunning tests natively on Windows..." -ForegroundColor Cyan
    $env:Path = [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User")
    
    try {
        Push-Location "quicklendx-contracts"
        & cargo test -p quicklendx-contracts test_withdraw_bid_matrix -- --nocapture
        if ($LASTEXITCODE -eq 0) {
            Write-Host "✓ Tests completed successfully!" -ForegroundColor Green
            Pop-Location
            return $true
        }
        Pop-Location
    } catch {
        Write-Host "✗ Native execution failed: $_" -ForegroundColor Red
        Pop-Location
    }
    return $false
}

# Main script
Write-Host "QuickLendX Protocol - withdraw_bid Test Runner" -ForegroundColor Cyan
Write-Host "============================================`n" -ForegroundColor Cyan

if ($InstallDeps) {
    Write-Host "Installing dependencies..." -ForegroundColor Yellow
    rustup update
    rustup target add wasm32-unknown-unknown
}

Write-Host "Checking prerequisites...`n"
$hasRust = Check-Rust
$hasTools = Check-BuildTools

if (-not $hasRust) {
    Write-Host "`n✗ Rust toolchain not available. Please install Rust from https://rustup.rs/" -ForegroundColor Red
    exit 1
}

if ($UseDocker) {
    if (Run-WithDocker) { exit 0 }
} elseif ($UseWSL) {
    if (Run-WithWSL) { exit 0 }
} else {
    # Try methods in order of reliability
    Write-Host "`nTrying execution methods in order (Docker > WSL > Native)...`n"
    
    # Try Docker first
    Write-Host "1. Attempting Docker..." -ForegroundColor Yellow
    if (Run-WithDocker) { exit 0 }
    
    # Try WSL second
    Write-Host "`n2. Attempting WSL..." -ForegroundColor Yellow
    if (Run-WithWSL) { exit 0 }
    
    # Try native Windows last
    Write-Host "`n3. Attempting native Windows..." -ForegroundColor Yellow
    if (-not $hasTools) {
        Write-Host "`n⚠ Build Tools not found. Installing is recommended for native Windows builds." -ForegroundColor Yellow
        Write-Host "Download from: https://aka.ms/vs/17/release/vs_BuildTools.exe" -ForegroundColor Cyan
    }
    if (Run-Native) { exit 0 }
}

Write-Host "`n✗ All execution methods failed." -ForegroundColor Red
Write-Host "`nTroubleshooting options:" -ForegroundColor Yellow
Write-Host "1. Use Docker: .\run-withdraw-bid-tests.ps1 -UseDocker" -ForegroundColor Cyan
Write-Host "2. Use WSL: .\run-withdraw-bid-tests.ps1 -UseWSL" -ForegroundColor Cyan
Write-Host "3. Install Build Tools: https://aka.ms/vs/17/release/vs_BuildTools.exe" -ForegroundColor Cyan
Write-Host "4. Refresh PATH: refreshenv (requires posh-git) or restart PowerShell" -ForegroundColor Cyan

exit 1
