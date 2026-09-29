#!/usr/bin/env pwsh
# Protocol Limits Boundary Feature - Git Operations Script for PowerShell

Write-Host "Creating feature branch: feature/protocol-limits-boundary-tests" -ForegroundColor Cyan
git checkout -b feature/protocol-limits-boundary-tests 2>$null
if ($LASTEXITCODE -ne 0) {
    Write-Host "Branch already exists, switching to it..." -ForegroundColor Yellow
    git checkout feature/protocol-limits-boundary-tests
}

Write-Host ""
Write-Host "Staging all changes..." -ForegroundColor Cyan
git add -A

Write-Host ""
Write-Host "Committing changes..." -ForegroundColor Cyan
$commitMessage = @"
test(limits): add exact-boundary coverage for protocol limits and per-business cap

- Added exact-boundary test coverage in test_protocol_limits_boundary.rs and test_max_invoices_per_business.rs
- Handled off-by-one errors and edge cases (amount==min, due_date==max, business at cap)
- Updated docs/contracts/protocol-limits.md with clear inclusivity guidelines
- Added /// inclusivity comments to ProtocolConfig in storage_types.rs
- Validated atomic application of set_protocol_config

Closes issue
"@

git commit -m $commitMessage

Write-Host ""
Write-Host "Pushing to remote..." -ForegroundColor Cyan
git push -u origin feature/protocol-limits-boundary-tests

Write-Host ""
Write-Host "✅ Done! Review the changes and create a pull request." -ForegroundColor Green