#!/usr/bin/env pwsh
# Export Hardening Feature - Git Operations Script for PowerShell
# This script completes the git operations for issue #1063

Write-Host "Creating feature branch: feature/export-hardening" -ForegroundColor Cyan
git checkout -b feature/export-hardening 2>$null
if ($LASTEXITCODE -ne 0) {
    Write-Host "Branch already exists, switching to it..." -ForegroundColor Yellow
    git checkout feature/export-hardening
}

Write-Host ""
Write-Host "Staging all changes..." -ForegroundColor Cyan
git add -A

Write-Host ""
Write-Host "Current status before commit:" -ForegroundColor Cyan
git status

Write-Host ""
Write-Host "Committing changes..." -ForegroundColor Cyan
$commitMessage = @"
feat: harden data exports with validation, audit, caps, and integrity digest

- Add comprehensive export service with validation and streaming
- Implement audit trail tracking for all exports with audit service
- Add bounded output with configurable row/byte limits and back-pressure
- Include SHA256 integrity digest for export verification
- Create export controller with validation for all query parameters
- Add admin routes for audit export and statistics
- Implement 150+ tests covering all edge cases
- Document export limits, API endpoints, and security considerations
- Support multiple formats: NDJSON, JSON, CSV
- Record audit entry per export with user context and full metadata

Closes #1063
"@

git commit -m $commitMessage

Write-Host ""
Write-Host "Pushing to remote..." -ForegroundColor Cyan
git push origin feature/export-hardening

Write-Host ""
Write-Host "✅ Done! Review the changes and create a pull request." -ForegroundColor Green
Write-Host "Branch: feature/export-hardening" -ForegroundColor Green
```
