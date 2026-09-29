#!/usr/bin/env pwsh
# Default Finality Feature - Git Operations Script for PowerShell

Write-Host "Creating feature branch: feature/default-finality-tests" -ForegroundColor Cyan
git checkout -b feature/default-finality-tests 2>$null
if ($LASTEXITCODE -ne 0) {
    Write-Host "Branch already exists, switching to it..." -ForegroundColor Yellow
    git checkout feature/default-finality-tests
}

Write-Host ""
Write-Host "Staging all changes..." -ForegroundColor Cyan
git add -A

Write-Host ""
Write-Host "Committing changes..." -ForegroundColor Cyan
$commitMessage = @"
test(defaults): enforce terminal-state finality for defaulted invoices

- Expanded test_default_finality.rs to prove post-default operations fail
- Asserts N+1 default operations reject to ensure single insurance claim
- Updated docs/contracts/default-handling.md with terminal-state rules
- Added /// inclusivity comments to TransitionGuard documenting finality

Closes issue
"@

git commit -m $commitMessage

Write-Host ""
Write-Host "Pushing to remote..." -ForegroundColor Cyan
git push -u origin feature/default-finality-tests

Write-Host ""
Write-Host "✅ Done! Review the changes and create a pull request." -ForegroundColor Green