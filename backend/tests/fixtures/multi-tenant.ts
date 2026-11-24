/**
 * Multi-tenant test fixtures for tenant isolation testing.
 *
 * Defines distinct tenant groups (businesses and investors) with isolated
 * records for invoices, bids, and exports. Used to verify that cross-tenant
 * access requests fail safely without leaking resource existence.
 *
 * Updated against the current backend contract:
 *  - Tenant identifiers are valid Stellar public keys (G + 55 base32 chars) so
 *    they pass the shared `stellarAddressSchema` query validators.
 *  - API keys match the current `ApiKey` domain model (src/models/api-key.ts).
 *  - Invoice / bid IDs are valid hex strings so they pass `hexStringSchema`.
 *  - Seed helpers create the SQLite schema and populate the invoice store.
 *
 * NOTE: this module intentionally does NOT import the bid/invoice controllers
 * or bidStore. It is required from inside the jest.mock factory for bidStore,
 * and importing those modules here would create a circular require.
 */

import { Invoice, InvoiceStatus, InvoiceCategory } from "../../src/types/contract";
import { Bid, BidStatus } from "../../src/types/contract";
import { ApiKey } from "../../src/models/api-key";
import { invoiceStore } from "../../src/services/invoiceStore";
import { getDatabase } from "../../src/lib/database";
import crypto from "crypto";

// ─── Tenant Identifiers (valid Stellar public keys) ─────────────────────────

export const TENANT_BUSINESS_A = `G${"A".repeat(55)}`;
export const TENANT_BUSINESS_B = `G${"B".repeat(55)}`;
export const TENANT_INVESTOR_A = `G${"C".repeat(55)}`;
export const TENANT_INVESTOR_B = `G${"D".repeat(55)}`;

// ─── Bearer tokens for each tenant ──────────────────────────────────────────
// Tokens use the `qlx_` prefix so API-key-authenticated writes are exempt from
// the browser CSRF check (src/middleware/csrf.ts). The test harness injects a
// custom verifier that maps these tokens to the fixture keys below.

export const TOKEN_BUSINESS_A = `qlx_tenant_business_a_${"0".repeat(20)}`;
export const TOKEN_BUSINESS_B = `qlx_tenant_business_b_${"0".repeat(20)}`;
export const TOKEN_INVESTOR_A = `qlx_tenant_investor_a_${"0".repeat(20)}`;
export const TOKEN_INVESTOR_B = `qlx_tenant_investor_b_${"0".repeat(20)}`;

// ─── API Keys for Each Tenant (current ApiKey model) ────────────────────────

function makeApiKey(
  token: string,
  tenant: string,
  name: string,
  scopes: string[]
): ApiKey {
  const now = Date.now();
  return {
    id: crypto.randomUUID(),
    name,
    prefix: token.slice(0, 15),
    scopes,
    created_by: tenant,
    created_at: new Date(now).toISOString(),
    last_used_at: null,
    expires_at: null,
    revoked: false,
    signing_secret_hash: null,
    prev_signing_secret_hash: null,
    prev_secret_expires_at: null,
    ownerId: tenant,
    permissions: [...scopes],
    status: "active",
    version: 1,
    createdAt: now,
    revokedAt: null,
    revokedBy: null,
  };
}

export const API_KEY_BUSINESS_A: ApiKey = makeApiKey(
  TOKEN_BUSINESS_A,
  TENANT_BUSINESS_A,
  "Business A API Key",
  ["read:invoices", "write:invoices", "read:bids"]
);

export const API_KEY_BUSINESS_B: ApiKey = makeApiKey(
  TOKEN_BUSINESS_B,
  TENANT_BUSINESS_B,
  "Business B API Key",
  ["read:invoices", "write:invoices", "read:bids"]
);

export const API_KEY_INVESTOR_A: ApiKey = makeApiKey(
  TOKEN_INVESTOR_A,
  TENANT_INVESTOR_A,
  "Investor A API Key",
  ["read:invoices", "write:bids", "read:bids"]
);

export const API_KEY_INVESTOR_B: ApiKey = makeApiKey(
  TOKEN_INVESTOR_B,
  TENANT_INVESTOR_B,
  "Investor B API Key",
  ["read:invoices", "write:bids", "read:bids"]
);

/** Token → fixture key map used by the injected API-key verifier. */
export const API_KEY_BY_TOKEN: Record<string, ApiKey> = {
  [TOKEN_BUSINESS_A]: API_KEY_BUSINESS_A,
  [TOKEN_BUSINESS_B]: API_KEY_BUSINESS_B,
  [TOKEN_INVESTOR_A]: API_KEY_INVESTOR_A,
  [TOKEN_INVESTOR_B]: API_KEY_INVESTOR_B,
};

// ─── Invoice Fixtures ────────────────────────────────────────────────────────

export const INVOICE_BUSINESS_A_1: Invoice = {
  id: "0xa1" + crypto.randomBytes(31).toString("hex"),
  business: TENANT_BUSINESS_A,
  amount: "100000",
  currency: "USD",
  due_date: Math.floor(Date.now() / 1000) + 86400 * 30,
  status: InvoiceStatus.Verified,
  description: "Business A Invoice 1",
  category: InvoiceCategory.Services,
  tags: ["tenant-a", "test"],
  metadata: {
    customer_name: "Customer of Business A",
    customer_address: "123 A Street",
    tax_id: "TAX-A-001",
    line_items: [],
    notes: "Confidential to Business A",
  },
  created_at: Math.floor(Date.now() / 1000) - 86400,
  updated_at: Math.floor(Date.now() / 1000),
  contract_version: 1,
  event_schema_version: 1,
  indexed_at: new Date().toISOString(),
};

export const INVOICE_BUSINESS_A_2: Invoice = {
  id: "0xa2" + crypto.randomBytes(31).toString("hex"),
  business: TENANT_BUSINESS_A,
  amount: "50000",
  currency: "USD",
  due_date: Math.floor(Date.now() / 1000) + 86400 * 60,
  status: InvoiceStatus.Pending,
  description: "Business A Invoice 2",
  category: InvoiceCategory.Products,
  tags: ["tenant-a", "pending"],
  metadata: {
    customer_name: "Another Customer of A",
    customer_address: "456 A Avenue",
    tax_id: "TAX-A-002",
    line_items: [],
    notes: "Private to Business A",
  },
  created_at: Math.floor(Date.now() / 1000) - 7200,
  updated_at: Math.floor(Date.now() / 1000),
  contract_version: 1,
  event_schema_version: 1,
  indexed_at: new Date().toISOString(),
};

export const INVOICE_BUSINESS_B_1: Invoice = {
  id: "0xb1" + crypto.randomBytes(31).toString("hex"),
  business: TENANT_BUSINESS_B,
  amount: "200000",
  currency: "USD",
  due_date: Math.floor(Date.now() / 1000) + 86400 * 45,
  status: InvoiceStatus.Verified,
  description: "Business B Invoice 1",
  category: InvoiceCategory.Technology,
  tags: ["tenant-b", "test"],
  metadata: {
    customer_name: "Customer of Business B",
    customer_address: "789 B Boulevard",
    tax_id: "TAX-B-001",
    line_items: [],
    notes: "Confidential to Business B",
  },
  created_at: Math.floor(Date.now() / 1000) - 86400,
  updated_at: Math.floor(Date.now() / 1000),
  contract_version: 1,
  event_schema_version: 1,
  indexed_at: new Date().toISOString(),
};

export const INVOICE_BUSINESS_B_2: Invoice = {
  id: "0xb2" + crypto.randomBytes(31).toString("hex"),
  business: TENANT_BUSINESS_B,
  amount: "75000",
  currency: "USD",
  due_date: Math.floor(Date.now() / 1000) + 86400 * 90,
  status: InvoiceStatus.Funded,
  description: "Business B Invoice 2",
  category: InvoiceCategory.Consulting,
  tags: ["tenant-b", "funded"],
  metadata: {
    customer_name: "Major Client of B",
    customer_address: "101 B Street",
    tax_id: "TAX-B-002",
    line_items: [],
    notes: "Private to Business B",
  },
  created_at: Math.floor(Date.now() / 1000) - 3600,
  updated_at: Math.floor(Date.now() / 1000),
  contract_version: 1,
  event_schema_version: 1,
  indexed_at: new Date().toISOString(),
};

// ─── Bid Fixtures (valid hex bid_id) ─────────────────────────────────────────

export const BID_INVESTOR_A_ON_BUSINESS_A: Bid = {
  bid_id: "0x" + "a1".repeat(32),
  invoice_id: INVOICE_BUSINESS_A_1.id,
  investor: TENANT_INVESTOR_A,
  bid_amount: "95000",
  expected_return: "100000",
  timestamp: Math.floor(Date.now() / 1000) - 3600,
  status: BidStatus.Placed,
  expiration_timestamp: Math.floor(Date.now() / 1000) + 86400 * 7,
  contract_version: 1,
  event_schema_version: 1,
  indexed_at: new Date().toISOString(),
};

export const BID_INVESTOR_B_ON_BUSINESS_B: Bid = {
  bid_id: "0x" + "b1".repeat(32),
  invoice_id: INVOICE_BUSINESS_B_1.id,
  investor: TENANT_INVESTOR_B,
  bid_amount: "190000",
  expected_return: "200000",
  timestamp: Math.floor(Date.now() / 1000) - 1800,
  status: BidStatus.Placed,
  expiration_timestamp: Math.floor(Date.now() / 1000) + 86400 * 14,
  contract_version: 1,
  event_schema_version: 1,
  indexed_at: new Date().toISOString(),
};

// Cross-tenant bid: Investor A bids on Business B's invoice
export const BID_INVESTOR_A_ON_BUSINESS_B: Bid = {
  bid_id: "0x" + "ab".repeat(32),
  invoice_id: INVOICE_BUSINESS_B_1.id,
  investor: TENANT_INVESTOR_A,
  bid_amount: "185000",
  expected_return: "195000",
  timestamp: Math.floor(Date.now() / 1000) - 900,
  status: BidStatus.Placed,
  expiration_timestamp: Math.floor(Date.now() / 1000) + 86400 * 10,
  contract_version: 1,
  event_schema_version: 1,
  indexed_at: new Date().toISOString(),
};

// ─── Exported Collections ────────────────────────────────────────────────────

export const ALL_TENANT_INVOICES = [
  INVOICE_BUSINESS_A_1,
  INVOICE_BUSINESS_A_2,
  INVOICE_BUSINESS_B_1,
  INVOICE_BUSINESS_B_2,
];

export const ALL_TENANT_BIDS = [
  BID_INVESTOR_A_ON_BUSINESS_A,
  BID_INVESTOR_B_ON_BUSINESS_B,
  BID_INVESTOR_A_ON_BUSINESS_B,
];

export const TENANT_A_INVOICES = [INVOICE_BUSINESS_A_1, INVOICE_BUSINESS_A_2];
export const TENANT_B_INVOICES = [INVOICE_BUSINESS_B_1, INVOICE_BUSINESS_B_2];

export const TENANT_A_BIDS = [BID_INVESTOR_A_ON_BUSINESS_A, BID_INVESTOR_A_ON_BUSINESS_B];
export const TENANT_B_BIDS = [BID_INVESTOR_B_ON_BUSINESS_B];

// ─── Seed Helpers ────────────────────────────────────────────────────────────

/**
 * Create the SQLite tables the tenant-isolation suite touches. The api_keys and
 * api_key_audit_log tables are required because the auth middleware records key
 * usage (updateLastUsed) on every authenticated request; without them that
 * fire-and-forget write would reject with "no such table".
 */
export function createTenantSchema(): void {
  const db = getDatabase();
  db.exec(`
    CREATE TABLE IF NOT EXISTS invoices (
      id TEXT PRIMARY KEY, business TEXT NOT NULL, amount TEXT NOT NULL,
      currency TEXT NOT NULL, due_date INTEGER NOT NULL, status TEXT NOT NULL,
      description TEXT NOT NULL, category TEXT NOT NULL, tags TEXT NOT NULL,
      metadata TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
      contract_version INTEGER NOT NULL, event_schema_version INTEGER NOT NULL,
      indexed_at TEXT
    );
    CREATE TABLE IF NOT EXISTS api_keys (
      id TEXT PRIMARY KEY,
      key_hash TEXT NOT NULL,
      signing_secret_hash TEXT,
      prev_signing_secret_hash TEXT,
      prefix TEXT NOT NULL,
      name TEXT NOT NULL,
      scopes TEXT NOT NULL,
      created_at TEXT NOT NULL,
      last_used_at TEXT,
      expires_at TEXT,
      prev_secret_expires_at TEXT,
      revoked INTEGER NOT NULL DEFAULT 0,
      created_by TEXT NOT NULL,
      version INTEGER NOT NULL DEFAULT 1,
      revoked_at INTEGER,
      revoked_by TEXT
    );
    CREATE TABLE IF NOT EXISTS api_key_audit_log (
      id TEXT PRIMARY KEY,
      event_type TEXT NOT NULL,
      key_id TEXT NOT NULL,
      actor TEXT NOT NULL,
      timestamp TEXT NOT NULL,
      ip_address TEXT,
      endpoint TEXT,
      metadata TEXT
    );
  `);
}

/** Insert all tenant invoices into the (isolated) SQLite invoice store. */
export function seedTenantInvoices(): void {
  for (const invoice of ALL_TENANT_INVOICES) {
    invoiceStore.insertInvoice(invoice);
  }
}