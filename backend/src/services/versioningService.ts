import { VersionedRecord } from "../types/contract";

/**
 * Matches PROTOCOL_VERSION in quicklendx-contracts/src/init.rs.
 * Bump this when the contract is upgraded and re-deployed.
 */
export const CURRENT_CONTRACT_VERSION = 1;

/**
 * Tracks the event payload shape. Increment when field positions or types
 * change in an incompatible way. New append-only fields do not require a bump.
 */
export const CURRENT_EVENT_SCHEMA_VERSION = 1;

/**
 * Maps each canonical event topic string (from docs/contracts/events.md)
 * to the event schema version that produced it.
 *
 * When an event topic's payload is extended in a breaking way, add a new
 * entry keyed by the new topic string and bump its version.
 */
export const EVENT_TOPIC_SCHEMA_VERSIONS: Readonly<Record<string, number>> = {
  invoice_created: 1,
  invoice_status_changed: 1,
  invoice_funded: 1,
  invoice_paid: 1,
  invoice_cancelled: 1,
  bid_placed: 1,
  bid_withdrawn: 1,
  bid_accepted: 1,
  bid_expired: 1,
  settlement_initiated: 1,
  settlement_paid: 1,
  settlement_defaulted: 1,
  dispute_raised: 1,
  dispute_reviewed: 1,
  dispute_resolved: 1,
};

/**
 * Returns the event schema version for a given topic string.
 * Falls back to CURRENT_EVENT_SCHEMA_VERSION for unknown topics so that
 * newly added events are handled gracefully without throwing.
 */
export function resolveEventSchemaVersion(eventTopic: string): number {
  return EVENT_TOPIC_SCHEMA_VERSIONS[eventTopic] ?? CURRENT_EVENT_SCHEMA_VERSION;
}

/**
 * Stamps a record with version labels at ingest time.
 *
 * Versions are always derived from the trusted on-chain event data or
 * the indexer's own constants — never from user-supplied request input.
 *
 * @param record - The raw record object to label.
 * @param contractVersion - The contract version that emitted this record.
 * @param eventSchemaVersion - The event schema version of the originating event.
 * @returns A new object with version labels merged in.
 */
export function labelRecord<T extends object>(
  record: T,
  contractVersion: number = CURRENT_CONTRACT_VERSION,
  eventSchemaVersion: number = CURRENT_EVENT_SCHEMA_VERSION
): T & VersionedRecord {
  if (!Number.isInteger(contractVersion) || contractVersion < 1) {
    throw new RangeError(`Invalid contractVersion: ${contractVersion}`);
  }
  if (!Number.isInteger(eventSchemaVersion) || eventSchemaVersion < 1) {
    throw new RangeError(`Invalid eventSchemaVersion: ${eventSchemaVersion}`);
  }
  return {
    ...record,
    contract_version: contractVersion,
    event_schema_version: eventSchemaVersion,
    indexed_at: new Date().toISOString(),
  };
}
