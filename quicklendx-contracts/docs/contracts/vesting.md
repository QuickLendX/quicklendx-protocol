# Vesting Contract Module

This module implements time-locked token vesting for protocol tokens or rewards.
Vesting schedules are created by the admin, funded upfront, and released linearly
over time after an optional cliff. Beneficiaries can claim vested tokens as they
unlock.

## Overview

- Admin creates a schedule with a token, beneficiary, total amount, start time,
  cliff seconds, and end time.
- Tokens are transferred into contract custody at schedule creation.
- Vesting is linear from `start_time` to `end_time`. Before the cliff, nothing
  can be released.
- Beneficiaries can release vested tokens in multiple claims until fully vested.

## Core Methods

- `create_vesting_schedule` (admin only)
- `get_vesting_schedule`
- `get_vested_amount`
- `get_vesting_releasable`
- `release_vested_tokens` (beneficiary only)

## Time Model

- `start_time`: When vesting begins.
- `cliff_time = start_time + cliff_seconds`: Earliest time any tokens can be
  released.
- `end_time`: When 100% of tokens are vested.
- Vesting before the cliff is always 0.
- Vesting after `end_time` is always the full amount.

## Security Notes

- Admin authorization is required to create schedules.
- Beneficiary authorization is required to release tokens.
- Tokens are moved into contract custody at creation to guarantee availability.
- Release can only occur when a positive amount is vested and unreleased.
- Timestamp validation prevents invalid or inverted schedules.

## Example Flow

1. Admin approves the contract to transfer `total_amount` tokens.
2. Admin calls `create_vesting_schedule`.
3. Beneficiary calls `release_vested_tokens` over time.

## Edge Cases

- Zero amount schedules are rejected.
- `end_time` must be strictly greater than `start_time`.
- Cliff time cannot exceed end time.
- Releasing before cliff or when nothing is releasable fails.
