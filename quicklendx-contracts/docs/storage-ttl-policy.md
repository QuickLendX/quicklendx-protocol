# Storage TTL Policy

This document outlines the persistent storage lifecycle management for the QuickLendX Protocol.

## Overview

To optimize storage usage and minimize the cost of managing large amounts of historical data on the Stellar network, certain persistent storage entries are subject to a Time-To-Live (TTL) policy.

## TTL Management

The protocol provides an admin-callable entrypoint to extend the TTL of all major persistent storage indexes. This ensures that active and relevant protocol data remains available.

### Admin Entrypoint

`extend_protocol_ttl(env, admin)`

This function iterates through the following root indexes and extends the TTL for each entry:

1.  **Invoices**: All existing invoice records.
2.  **Bids**: All existing bid records.
3.  **Investments**: All active investment records.
4.  **Escrows**: All existing escrow records (found via their relationship to invoices).
5.  **Currency Whitelist**: All whitelisted currency addresses.

### Operational Schedule

It is recommended that this function be called **weekly** by the protocol administrator or an automated service to prevent accidental data expiration.

### Monitoring

The extension process emits `TtlExtended` events for each type of data refreshed. Off-chain monitoring services should subscribe to these events to track the health of the storage lifecycle management.

## Risk Note

Failure to run the TTL extension regularly may result in the loss of persistent data as the Soroban host reclaims expired storage. This can lead to broken lookups and protocol state inconsistencies.
