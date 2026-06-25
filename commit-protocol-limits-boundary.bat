@echo off
REM Protocol Limits Boundary Feature - Git Operations Script

echo Creating feature branch: feature/protocol-limits-boundary-tests
git checkout -b feature/protocol-limits-boundary-tests || git checkout feature/protocol-limits-boundary-tests

echo Staging all changes...
git add -A

echo Committing changes...
git commit -m "test(limits): add exact-boundary coverage for protocol limits and per-business cap" -m "- Added exact-boundary test coverage in test_protocol_limits_boundary.rs and test_max_invoices_per_business.rs" -m "- Handled off-by-one errors and edge cases (amount==min, due_date==max, business at cap)" -m "- Updated docs/contracts/protocol-limits.md with clear inclusivity guidelines" -m "- Added /// inclusivity comments to ProtocolConfig in storage_types.rs" -m "- Validated atomic application of set_protocol_config"

echo Pushing to remote...
git push -u origin feature/protocol-limits-boundary-tests

echo.
echo Done! Review the changes and create a pull request.
echo Branch: feature/protocol-limits-boundary-tests