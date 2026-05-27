@echo off
REM Export Hardening Feature - Git Operations Script
REM This script completes the git operations for issue #1063

echo Creating feature branch: feature/export-hardening
git checkout -b feature/export-hardening || git checkout feature/export-hardening

echo.
echo Staging all changes...
git add -A

echo.
echo Current status before commit:
git status

echo.
echo Committing changes...
git commit -m "feat: harden data exports with validation, audit, caps, and integrity digest

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

Closes #1063"

echo.
echo Pushing to remote...
git push origin feature/export-hardening

echo.
echo Done! Review the changes and create a pull request.
echo Branch: feature/export-hardening
