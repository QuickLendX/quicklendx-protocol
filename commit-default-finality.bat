@echo off
REM Default Finality Feature - Git Operations Script

echo Creating feature branch: feature/default-finality-tests
git checkout -b feature/default-finality-tests || git checkout feature/default-finality-tests

echo Staging all changes...
git add -A

echo Committing changes...
git commit -m "test(defaults): enforce terminal-state finality for defaulted invoices" -m "- Expanded test_default_finality.rs to prove post-default operations fail" -m "- Asserts N+1 default operations reject to ensure single insurance claim" -m "- Updated docs/contracts/default-handling.md with terminal-state rules" -m "- Added /// inclusivity comments to TransitionGuard documenting finality"

echo Pushing to remote...
git push -u origin feature/default-finality-tests

echo.
echo Done! Review the changes and create a pull request.
echo Branch: feature/default-finality-tests