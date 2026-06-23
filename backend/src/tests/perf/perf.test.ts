/**
 * Performance regression tests for database operations.
 * 
 * Tests verify that the prepared statement cache and SQLite pragmas
 * provide measurable performance improvements over naive implementations.
 */

import { getDatabase, getPreparedStatement, closeDatabase, clearStatementCache, getStatementCacheStats } from '../../lib/database';
import { invoiceStore } from '../../services/invoiceStore';
import { db as apiKeyDb } from '../../db/database';
import { Invoice, InvoiceStatus } from '../../types/contract';
import { ulid } from 'ulid';

describe('Database Performance Tests', () => {
  const TEST_DB = ':memory:';
  
  beforeAll(() => {
    process.env.DATABASE_PATH = TEST_DB;
  });

  beforeEach(() => {
    closeDatabase();
    clearStatementCache();
    const conn = getDatabase();
    
    // Create invoices table
    conn.exec(`
      CREATE TABLE IF NOT EXISTS invoices (
        id TEXT PRIMARY KEY,
        business TEXT NOT NULL,
        amount TEXT NOT NULL,
        currency TEXT NOT NULL,
        due_date INTEGER NOT NULL,
        status TEXT NOT NULL,
        description TEXT NOT NULL,
        category TEXT NOT NULL,
        tags TEXT NOT NULL,
        metadata TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        contract_version INTEGER NOT NULL,
        event_schema_version INTEGER NOT NULL,
        indexed_at TEXT NOT NULL
      );
      CREATE INDEX IF NOT EXISTS idx_invoices_business ON invoices(business);
      CREATE INDEX IF NOT EXISTS idx_invoices_status ON invoices(status);
    `);

    // Create API keys tables
    conn.exec(`
      CREATE TABLE IF NOT EXISTS api_keys (
        id TEXT PRIMARY KEY,
        key_hash TEXT NOT NULL,
        prefix TEXT UNIQUE NOT NULL,
        name TEXT NOT NULL,
        scopes TEXT NOT NULL,
        created_at TEXT NOT NULL,
        last_used_at TEXT,
        expires_at TEXT,
        revoked INTEGER DEFAULT 0,
        created_by TEXT NOT NULL,
        prev_signing_secret_hash TEXT,
        prev_secret_expires_at TEXT
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
  });

  afterAll(() => {
    closeDatabase();
  });

  describe('Statement Cache', () => {
    it('should cache prepared statements', () => {
      const sql = 'SELECT * FROM invoices WHERE id = ?';
      
      // First call should prepare and cache
      const stmt1 = getPreparedStatement(sql);
      const stats1 = getStatementCacheStats();
      expect(stats1.size).toBe(1);
      expect(stats1.statements).toContain(sql);

      // Second call should return cached statement
      const stmt2 = getPreparedStatement(sql);
      const stats2 = getStatementCacheStats();
      expect(stats2.size).toBe(1);
      expect(stmt1).toBe(stmt2); // Same object reference
    });

    it('should clear statement cache', () => {
      getPreparedStatement('SELECT * FROM invoices WHERE id = ?');
      getPreparedStatement('SELECT * FROM api_keys WHERE id = ?');
      
      expect(getStatementCacheStats().size).toBe(2);
      
      clearStatementCache();
      expect(getStatementCacheStats().size).toBe(0);
    });
  });

  describe('Performance Benchmarks', () => {
    const createTestInvoice = (id?: string): Invoice => ({
      id: id || ulid(),
      business: 'GBUSINESS123',
      amount: '10000',
      currency: 'USDC',
      due_date: Date.now() + 86400000,
      status: InvoiceStatus.Verified,
      description: 'Test invoice',
      category: 'services' as any,
      tags: ['test'],
      metadata: { customer_name: 'Test', customer_address: 'Test Addr', tax_id: '123', line_items: [], notes: '' },
      created_at: Date.now(),
      updated_at: Date.now(),
      contract_version: 1,
      event_schema_version: 1,
      indexed_at: new Date().toISOString(),
    });

    it('should demonstrate cached statement performance advantage', () => {
      // Seed some invoices
      const invoiceCount = 100;
      for (let i = 0; i < invoiceCount; i++) {
        invoiceStore.insertInvoice(createTestInvoice());
      }

      // Create a specific invoice to query
      const targetId = ulid();
      invoiceStore.insertInvoice(createTestInvoice(targetId));

      // Warm up cache
      invoiceStore.findInvoiceById(targetId);

      // Benchmark with cached statements
      const cachedStart = process.hrtime.bigint();
      for (let i = 0; i < 1000; i++) {
        invoiceStore.findInvoiceById(targetId);
      }
      const cachedElapsed = Number(process.hrtime.bigint() - cachedStart) / 1e6; // ms

      // Benchmark without cache (simulate by using direct prepare)
      const db = getDatabase();
      const uncachedStart = process.hrtime.bigint();
      for (let i = 0; i < 1000; i++) {
        db.prepare('SELECT * FROM invoices WHERE id = ?').get(targetId);
      }
      const uncachedElapsed = Number(process.hrtime.bigint() - uncachedStart) / 1e6; // ms

      console.log(`\n📊 Statement Cache Performance:`);
      console.log(`   Cached:   ${cachedElapsed.toFixed(2)}ms for 1000 queries`);
      console.log(`   Uncached: ${uncachedElapsed.toFixed(2)}ms for 1000 queries`);
      console.log(`   Speedup:  ${(uncachedElapsed / cachedElapsed).toFixed(2)}x faster`);

      // Cached should be at least 10% faster (conservative estimate)
      expect(cachedElapsed).toBeLessThan(uncachedElapsed * 0.9);
    });

    it('should handle high-volume inserts efficiently', () => {
      const insertCount = 500;
      const start = process.hrtime.bigint();

      for (let i = 0; i < insertCount; i++) {
        invoiceStore.insertInvoice(createTestInvoice());
      }

      const elapsed = Number(process.hrtime.bigint() - start) / 1e6; // ms
      const avgPerInsert = elapsed / insertCount;

      console.log(`\n📊 Bulk Insert Performance:`);
      console.log(`   ${insertCount} inserts in ${elapsed.toFixed(2)}ms`);
      console.log(`   Average: ${avgPerInsert.toFixed(3)}ms per insert`);

      // Should average less than 5ms per insert (very conservative)
      expect(avgPerInsert).toBeLessThan(5);
    });

    it('should handle concurrent read operations efficiently', async () => {
      // Seed data
      const invoices = Array.from({ length: 50 }, () => createTestInvoice());
      invoices.forEach(inv => invoiceStore.insertInvoice(inv));

      // Simulate concurrent reads
      const readCount = 100;
      const start = process.hrtime.bigint();

      const promises = Array.from({ length: readCount }, (_, i) => {
        return Promise.resolve(invoiceStore.findInvoiceById(invoices[i % invoices.length].id));
      });

      await Promise.all(promises);

      const elapsed = Number(process.hrtime.bigint() - start) / 1e6; // ms
      const avgPerRead = elapsed / readCount;

      console.log(`\n📊 Concurrent Read Performance:`);
      console.log(`   ${readCount} concurrent reads in ${elapsed.toFixed(2)}ms`);
      console.log(`   Average: ${avgPerRead.toFixed(3)}ms per read`);

      // Should average less than 2ms per read
      expect(avgPerRead).toBeLessThan(2);
    });

    it('should handle complex filtered queries efficiently', () => {
      // Seed invoices with different statuses
      const statuses = [InvoiceStatus.Verified, InvoiceStatus.Funded, InvoiceStatus.Pending];
      for (let i = 0; i < 150; i++) {
        const invoice = createTestInvoice();
        invoice.status = statuses[i % statuses.length];
        invoice.business = `BUSINESS${i % 3}`;
        invoiceStore.insertInvoice(invoice);
      }

      // Benchmark filtered queries
      const iterations = 500;
      const start = process.hrtime.bigint();

      for (let i = 0; i < iterations; i++) {
        invoiceStore.findInvoices({ 
          business: 'BUSINESS1', 
          status: InvoiceStatus.Verified 
        });
      }

      const elapsed = Number(process.hrtime.bigint() - start) / 1e6; // ms
      const avgPerQuery = elapsed / iterations;

      console.log(`\n📊 Filtered Query Performance:`);
      console.log(`   ${iterations} filtered queries in ${elapsed.toFixed(2)}ms`);
      console.log(`   Average: ${avgPerQuery.toFixed(3)}ms per query`);

      // Should average less than 1ms per filtered query
      expect(avgPerQuery).toBeLessThan(1);
    });

    it('should verify WAL mode is enabled', () => {
      const db = getDatabase();
      const result = db.pragma('journal_mode', { simple: true });
      expect(result).toBe('wal');
      console.log(`\n✅ WAL mode: ${result}`);
    });

    it('should verify synchronous mode is NORMAL', () => {
      const db = getDatabase();
      const result = db.pragma('synchronous', { simple: true });
      expect(result).toBe(1); // NORMAL = 1
      console.log(`✅ Synchronous mode: ${result === 1 ? 'NORMAL' : result}`);
    });

    it('should verify busy_timeout is configured', () => {
      const db = getDatabase();
      const result = db.pragma('busy_timeout', { simple: true });
      expect(result).toBe(5000);
      console.log(`✅ Busy timeout: ${result}ms`);
    });
  });

  describe('Edge Cases', () => {
    it('should handle schema changes gracefully', () => {
      const sql = 'SELECT * FROM invoices WHERE id = ?';
      getPreparedStatement(sql);
      
      expect(getStatementCacheStats().size).toBe(1);
      
      // Simulate schema change scenario - clear cache
      clearStatementCache();
      
      // Should be able to prepare again
      const stmt = getPreparedStatement(sql);
      expect(stmt).toBeDefined();
      expect(getStatementCacheStats().size).toBe(1);
    });

    it('should handle concurrent statement preparation safely', async () => {
      const sql = 'SELECT * FROM invoices WHERE id = ?';
      
      // Simulate concurrent requests for the same statement
      const promises = Array.from({ length: 10 }, () => {
        return Promise.resolve(getPreparedStatement(sql));
      });

      const statements = await Promise.all(promises);
      
      // All should reference the same cached statement
      const firstStmt = statements[0];
      statements.forEach(stmt => {
        expect(stmt).toBe(firstStmt);
      });
      
      // Only one entry in cache
      expect(getStatementCacheStats().size).toBe(1);
    });

    it('should handle parameterized queries securely', () => {
      const maliciousId = "'; DROP TABLE invoices; --";
      
      // Should not throw or execute malicious SQL
      const result = invoiceStore.findInvoiceById(maliciousId);
      expect(result).toBeUndefined();
      
      // Table should still exist
      const db = getDatabase();
      expect(() => {
        db.prepare('SELECT COUNT(*) FROM invoices').get();
      }).not.toThrow();
    });

    it('should handle empty result sets efficiently', () => {
      const iterations = 1000;
      const start = process.hrtime.bigint();

      for (let i = 0; i < iterations; i++) {
        invoiceStore.findInvoiceById('NONEXISTENT-ID');
      }

      const elapsed = Number(process.hrtime.bigint() - start) / 1e6; // ms
      expect(elapsed).toBeLessThan(500); // Should complete in under 500ms
    });
  });

  describe('API Key Store Performance', () => {
    it('should efficiently handle API key lookups', () => {
      // Seed API keys
      for (let i = 0; i < 100; i++) {
        apiKeyDb.createApiKey({
          id: ulid(),
          key_hash: `hash_${i}`,
          prefix: `prefix_${i}`,
          name: `Key ${i}`,
          scopes: 'read,write',
          created_at: new Date().toISOString(),
          last_used_at: null,
          expires_at: null,
          prev_secret_expires_at: null,
          prev_signing_secret_hash: null,
          revoked: 0,
          created_by: 'admin',
        });
      }

      // Benchmark prefix lookups
      const iterations = 500;
      const start = process.hrtime.bigint();

      for (let i = 0; i < iterations; i++) {
        apiKeyDb.getApiKeyByPrefix(`prefix_${i % 100}`);
      }

      const elapsed = Number(process.hrtime.bigint() - start) / 1e6;
      const avgPerLookup = elapsed / iterations;

      console.log(`\n📊 API Key Lookup Performance:`);
      console.log(`   ${iterations} lookups in ${elapsed.toFixed(2)}ms`);
      console.log(`   Average: ${avgPerLookup.toFixed(3)}ms per lookup`);

      expect(avgPerLookup).toBeLessThan(0.5);
    });
  });
});
