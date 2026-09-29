# Persistence

This document outlines the persistent storage structures for the QuickLendX Protocol backend.

## Invoices Table

The `invoices` table materializes the event stream from the Soroban contracts into a queryable relational state using `better-sqlite3`.

### Schema

| Column | Type | Constraints | Description |
| :--- | :--- | :--- | :--- |
| `id` | TEXT | PRIMARY KEY | The unique identifier of the invoice. |
| `business` | TEXT | NOT NULL | The Stellar address of the business. |
| `amount` | TEXT | NOT NULL | The total invoice amount. |
| `currency` | TEXT | NOT NULL | The currency of the invoice. |
| `due_date` | INTEGER | NOT NULL | The Unix timestamp when the invoice is due. |
| `status` | TEXT | NOT NULL | Current status of the invoice (e.g., Pending, Verified). |
| `description` | TEXT | NOT NULL | A short description. |
| `category` | TEXT | NOT NULL | The category of the invoice. |
| `tags` | TEXT | NOT NULL | JSON serialized array of tag strings. |
| `metadata` | TEXT | NOT NULL | JSON serialized object containing additional invoice details. |
| `created_at` | INTEGER | NOT NULL | Unix timestamp of creation. |
| `updated_at` | INTEGER | NOT NULL | Unix timestamp of last update. |
| `contract_version` | INTEGER | NOT NULL | Version of the contract that produced the event. |
| `event_schema_version` | INTEGER | NOT NULL | Version of the event schema. |
| `indexed_at` | TEXT | NOT NULL | ISO 8601 timestamp indicating when this record was indexed. |

### Indexes

- `idx_invoices_business`: Index on the `business` column to quickly fetch invoices for a specific business.
- `idx_invoices_status`: Index on the `status` column to filter invoices by their current status.

## Store API

The data is interacted with using `src/services/invoiceStore.ts`, which provides the following methods:

- `findInvoices(filter)`: Returns a list of invoices, optionally filtered by `business` and/or `status`.
- `findInvoiceById(id)`: Returns a single invoice matching the given `id`, or undefined if not found.
- `insertInvoice(invoice)`: Inserts a complete invoice record into the table.
- `deleteAll()`: Clears the table (primarily used for testing or reset operations).
