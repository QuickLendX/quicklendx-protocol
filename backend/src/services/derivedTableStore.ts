import { createHash } from "crypto";
import { DerivedTableStore } from "../types/replay";

// In-memory derived tables for testing and development
interface InMemoryTables {
  invoices: Map<string, any>;
  bids: Map<string, any>;
  settlements: Map<string, any>;
  disputes: Map<string, any>;
  notifications: Map<string, any>;
}

export class InMemoryDerivedTableStore implements DerivedTableStore {
  private tables: InMemoryTables;
  private transactionSnapshot: InMemoryTables | null = null;
  private inTransaction = false;

  constructor() {
    this.tables = {
      invoices: new Map(),
      bids: new Map(),
      settlements: new Map(),
      disputes: new Map(),
      notifications: new Map(),
    };
  }

  async clearDerivedTables(): Promise<void> {
    if (this.inTransaction) {
      throw new Error("Cannot clear tables during transaction");
    }
    
    this.tables.invoices.clear();
    this.tables.bids.clear();
    this.tables.settlements.clear();
    this.tables.disputes.clear();
    this.tables.notifications.clear();
  }

  async getStateHash(): Promise<string> {
    const state = {
      invoices: Array.from(this.tables.invoices.entries()),
      bids: Array.from(this.tables.bids.entries()),
      settlements: Array.from(this.tables.settlements.entries()),
      disputes: Array.from(this.tables.disputes.entries()),
      notifications: Array.from(this.tables.notifications.entries()),
    };
    
    const stateString = JSON.stringify(state, Object.keys(state).sort());
    return createHash("sha256").update(stateString).digest("hex");
  }

  async beginTransaction(): Promise<void> {
    if (this.inTransaction) {
      throw new Error("Transaction already in progress");
    }
    
    this.transactionSnapshot = {
      invoices: new Map(this.tables.invoices),
      bids: new Map(this.tables.bids),
      settlements: new Map(this.tables.settlements),
      disputes: new Map(this.tables.disputes),
      notifications: new Map(this.tables.notifications),
    };
    this.inTransaction = true;
  }

  async commitTransaction(): Promise<void> {
    if (!this.inTransaction) {
      throw new Error("No transaction in progress");
    }
    
    this.transactionSnapshot = null;
    this.inTransaction = false;
  }

  async rollbackTransaction(): Promise<void> {
    if (!this.inTransaction || !this.transactionSnapshot) {
      throw new Error("No transaction to rollback");
    }
    
    this.tables = {
      invoices: new Map(this.transactionSnapshot.invoices),
      bids: new Map(this.transactionSnapshot.bids),
      settlements: new Map(this.transactionSnapshot.settlements),
      disputes: new Map(this.transactionSnapshot.disputes),
      notifications: new Map(this.transactionSnapshot.notifications),
    };
    
    this.transactionSnapshot = null;
    this.inTransaction = false;
  }

  // Direct table access methods for event processor
  async upsertInvoice(invoice: any): Promise<void> {
    this.tables.invoices.set(invoice.id, invoice);
  }

  async upsertBid(bid: any): Promise<void> {
    this.tables.bids.set(bid.bid_id, bid);
  }

  async upsertSettlement(settlement: any): Promise<void> {
    this.tables.settlements.set(settlement.id, settlement);
  }

  async upsertDispute(dispute: any): Promise<void> {
    this.tables.disputes.set(dispute.id, dispute);
  }

  async upsertNotification(notification: any): Promise<void> {
    this.tables.notifications.set(notification.id, notification);
  }

  // Query methods for testing
  async getInvoice(id: string): Promise<any | null> {
    return this.tables.invoices.get(id) || null;
  }

  async getBid(id: string): Promise<any | null> {
    return this.tables.bids.get(id) || null;
  }

  async getSettlement(id: string): Promise<any | null> {
    return this.tables.settlements.get(id) || null;
  }

  async getDispute(id: string): Promise<any | null> {
    return this.tables.disputes.get(id) || null;
  }

  async getNotification(id: string): Promise<any | null> {
    return this.tables.notifications.get(id) || null;
  }

  // Test helpers
  getTableCounts(): { invoices: number; bids: number; settlements: number; disputes: number; notifications: number } {
    return {
      invoices: this.tables.invoices.size,
      bids: this.tables.bids.size,
      settlements: this.tables.settlements.size,
      disputes: this.tables.disputes.size,
      notifications: this.tables.notifications.size,
    };
  }

  reset(): void {
    if (this.inTransaction) {
      throw new Error("Cannot reset during transaction");
    }
    
    this.tables.invoices.clear();
    this.tables.bids.clear();
    this.tables.settlements.clear();
    this.tables.disputes.clear();
    this.tables.notifications.clear();
  }
}

// File-based derived table store for persistence
export class FileSystemDerivedTableStore implements DerivedTableStore {
  private readonly dataDir: string;
  private readonly tablesFiles = {
    invoices: "invoices.jsonl",
    bids: "bids.jsonl", 
    settlements: "settlements.jsonl",
    disputes: "disputes.jsonl",
    notifications: "notifications.jsonl",
  };
  private transactionBackupDir: string | null = null;

  constructor(dataDir?: string) {
    this.dataDir = dataDir || require("path").join(require("process").cwd(), ".data", "derived-tables");
  }

  async clearDerivedTables(): Promise<void> {
    const fs = require("fs").promises;
    const path = require("path");
    
    try {
      await fs.rm(this.dataDir, { recursive: true, force: true });
    } catch {
      // Ignore errors if directory doesn't exist
    }
  }

  async getStateHash(): Promise<string> {
    const fs = require("fs").promises;
    const path = require("path");
    
    const state: any = {};
    
    for (const [table, filename] of Object.entries(this.tablesFiles)) {
      const filePath = path.join(this.dataDir, filename);
      try {
        const data = await fs.readFile(filePath, "utf8");
        const lines = data.trim().split("\n").filter((line: string) => line.length > 0);
        state[table] = lines;
      } catch (error: any) {
        if (error.code === "ENOENT") {
          state[table] = [];
        } else {
          throw error;
        }
      }
    }
    
    const stateString = JSON.stringify(state, Object.keys(state).sort());
    return createHash("sha256").update(stateString).digest("hex");
  }

  async beginTransaction(): Promise<void> {
    if (this.transactionBackupDir) {
      throw new Error("Transaction already in progress");
    }
    
    const fs = require("fs").promises;
    const path = require("path");
    
    // Create backup directory
    const timestamp = Date.now();
    this.transactionBackupDir = path.join(this.dataDir, `.transaction-backup-${timestamp}`);
    
    try {
      // Copy all table files to backup
      await fs.mkdir(this.transactionBackupDir, { recursive: true });
      
      for (const [table, filename] of Object.entries(this.tablesFiles)) {
        const sourcePath = path.join(this.dataDir, filename);
        const backupPath = path.join(this.transactionBackupDir, filename);
        
        try {
          await fs.copyFile(sourcePath, backupPath);
        } catch (error: any) {
          if (error.code !== "ENOENT") {
            throw error;
          }
        }
      }
    } catch (error) {
      this.transactionBackupDir = null;
      throw error;
    }
  }

  async commitTransaction(): Promise<void> {
    if (!this.transactionBackupDir) {
      throw new Error("No transaction in progress");
    }
    
    const fs = require("fs").promises;
    
    // Remove backup directory
    try {
      await fs.rm(this.transactionBackupDir, { recursive: true, force: true });
    } finally {
      this.transactionBackupDir = null;
    }
  }

  async rollbackTransaction(): Promise<void> {
    if (!this.transactionBackupDir) {
      throw new Error("No transaction to rollback");
    }
    
    const fs = require("fs").promises;
    const path = require("path");
    
    try {
      // Clear current tables
      await this.clearDerivedTables();
      
      // Restore from backup
      await fs.mkdir(this.dataDir, { recursive: true });
      
      for (const [table, filename] of Object.entries(this.tablesFiles)) {
        const backupPath = path.join(this.transactionBackupDir, filename);
        const targetPath = path.join(this.dataDir, filename);
        
        try {
          await fs.copyFile(backupPath, targetPath);
        } catch (error: any) {
          if (error.code !== "ENOENT") {
            throw error;
          }
        }
      }
    } finally {
      // Clean up backup directory
      try {
        await fs.rm(this.transactionBackupDir, { recursive: true, force: true });
      } finally {
        this.transactionBackupDir = null;
      }
    }
  }

  // Direct table access methods
  async upsertRecord(table: keyof typeof this.tablesFiles, record: any): Promise<void> {
    const fs = require("fs").promises;
    const path = require("path");
    
    await fs.mkdir(this.dataDir, { recursive: true });
    const filePath = path.join(this.dataDir, this.tablesFiles[table]);
    
    const line = JSON.stringify(record) + "\n";
    await fs.appendFile(filePath, line, "utf8");
  }

  // Test helpers
  async getRecordCount(table: keyof typeof this.tablesFiles): Promise<number> {
    const fs = require("fs").promises;
    const path = require("path");
    
    const filePath = path.join(this.dataDir, this.tablesFiles[table]);
    try {
      const data = await fs.readFile(filePath, "utf8");
      const lines = data.trim().split("\n").filter((line: string) => line.length > 0);
      return lines.length;
    } catch (error: any) {
      if (error.code === "ENOENT") {
        return 0;
      }
      throw error;
    }
  }
}
