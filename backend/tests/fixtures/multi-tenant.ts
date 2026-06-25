/**
 * Multi-tenant test fixtures for tenant isolation testing.
 * 
 * Defines distinct tenant groups (businesses and investors) with isolated
 * mock records for invoices, bids, and exports. Used to verify that cross-tenant
 * access requests fail safely without leaking resource existence.
 */

import { Invoice, InvoiceStatus, InvoiceCategory } from "../../src/types/contract";
import { Bid, BidStatus } from "../../src/types/contract";
import { ApiKey } from "../../src/models/api-key";
import crypto from "crypto";

// ─── Tenant Identifiers ──────────────────────────────────────────────────────

export const TENANT_BUSINESS_A = "GBUSINESS_A_STELLAR_ADDRESS_AAAAAAAAAAAAAAAAAAA";
export const TENANT_BUSINESS_B = "GBUSINESS_B_STELLAR_ADDRESS_BBBBBBBBBBBBBBBBBBB";
export const TENANT_INVESTOR_A = "GINVESTOR_A_STELLAR_ADDRESS_AAAAAAAAAAAAAAAAAAA";
export const TENANT_INVESTOR_B = "GINVESTOR_B_STELLAR_ADDRESS_BBBBBBBBBBBBBBBBBBB";

// ─── API Keys for Each Tenant ────────────────────────────────────────────────

export const API_KEY_BUSINESS_A: ApiKey = {
  id: crypto.randomUUID(),
  key_hash: crypto.createHash("sha256").update("business_a_key").digest("hex"),
  signing_secret_hash: null,
  prev_signing_secret_hash: null,
  prefix: "qlx_test_busA",
  name: "Business A API Key",
  scopes: ["read:invoices", "write:invoices", "read:bids"],
  created_at: new Date().toISOString(),
  last_used_at: null,
  expires_at: null,
  prev_secret_expires_at: null,
  revoked: false,
  created_by: TENANT_BUSINESS_A,
};

export const API_KEY_BUSINESS_B: ApiKey = {
  id: crypto.randomUUID(),
  key_hash: crypto.createHash("sha256").update("business_b_key").digest("hex"),
  signing_secret_hash: null,
  prev_signing_secret_hash: null,
  prefix: "qlx_test_busB",
  name: "Business B API Key",
  scopes: ["read:invoices", "write:invoices", "read:bids"],
  created_at: new Date().toISOString(),
  last_used_at: null,
  expires_at: null,
  prev_secret_expires_at: null,
  revoked: false,
  created_by: TENANT_BUSINESS_B,
};

export const API_KEY_INVESTOR_A: ApiKey = {
  id: crypto.randomUUID(),
  key_hash: crypto.createHash("sha256").update("investor_a_key").digest("hex"),
  signing_secret_hash: null,
  prev_signing_secret_hash: null,
  prefix: "qlx_test_invA",
  name: "Investor A API Key",
  scopes: ["read:invoices", "write:bids", "read:bids"],
  created_at: new Date().toISOString(),
  last_used_at: null,
  expires_at: null,
  prev_secret_expires_at: null,
  revoked: false,
  created_by: TENANT_INVESTOR_A,
};

export const API_KEY_INVESTOR_B: ApiKey = {
  id: crypto.randomUUID(),
  key_hash: crypto.createHash("sha256").update("investor_b_key").digest("hex"),
  signing_secret_hash: null,
  prev_signing_secret_hash: null,
  prefix: "qlx_test_invB",
  name: "Investor B API Key",
  scopes: ["read:invoices", "write:bids", "read:bids"],
  created_at: new Date().toISOString(),
  last_used_at: null,
  expires_at: null,
  prev_secret_expires_at: null,
  revoked: false,
  created_by: TENANT_INVESTOR_B,
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

// ─── Bid Fixtures ────────────────────────────────────────────────────────────

export const BID_INVESTOR_A_ON_BUSINESS_A: Bid = {
  bid_id: "0xbida1" + crypto.randomBytes(29).toString("hex"),
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
  bid_id: "0xbidb1" + crypto.randomBytes(29).toString("hex"),
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
  bid_id: "0xbidab" + crypto.randomBytes(29).toString("hex"),
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
