# Emergency Pause / Unpause

## Overview
The protocol includes an emergency pause flag to halt all state-changing operations during incidents. When paused, mutating entrypoints reject execution with `ProtocolPaused`. Read-only queries remain available.

## Admin Control
- `pause(admin)`: Sets the global pause flag to `true`.
- `unpause(admin)`: Clears the global pause flag.
- `is_paused()`: Read-only query for the current pause state.

Only the contract admin (set via `initialize_admin`) can pause or unpause. Calls require admin authorization.

## Impact When Paused
Blocked operations include:
- Invoice lifecycle changes (upload, verify, cancel, update status/metadata/tags/categories)
- Bidding and funding actions (place/accept/withdraw bids, escrow flows)
- Settlement and default handling
- KYC and verification updates
- Notifications and analytics updates
- Backup/restore and fee/revenue configuration

Read-only queries (e.g., `get_*` functions) are still allowed.

## Recovery
1. Investigate and remediate the incident off-chain.
2. Use `unpause(admin)` to resume normal operations.
3. Monitor critical flows (bids, escrow, settlement) after resuming.

## Security Notes
- The pause flag is stored in instance storage under a dedicated key.
- All mutating entrypoints must call `require_not_paused()` at the start of execution.
- Use pause only for emergencies; it is not intended for routine maintenance.
