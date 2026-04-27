import path from "path";
import { promises as fs } from "fs";
import { 
  RawEvent, 
  RawEventStore, 
  EventValidator,
  ReplayAuditEntry 
} from "../types/replay";
import { auditLogService } from "./auditLogService";

export class FileSystemRawEventStore implements RawEventStore {
  private readonly dataDir: string;
  private readonly eventsFile: string;
  private readonly cursorFile: string;
  private readonly maxFileSize: number;
  private readonly validator: EventValidator;

  constructor(
    validator: EventValidator,
    dataDir?: string,
    maxFileSize: number = 100 * 1024 * 1024 // 100MB per file
  ) {
    this.dataDir = dataDir || path.join(process.cwd(), ".data", "raw-events");
    this.eventsFile = path.join(this.dataDir, "events.jsonl");
    this.cursorFile = path.join(this.dataDir, "replay-cursor.json");
    this.maxFileSize = maxFileSize;
    this.validator = validator;
  }

  async storeEvents(events: RawEvent[]): Promise<void> {
    await fs.mkdir(this.dataDir, { recursive: true });
    
    // Validate and sanitize all events
    const sanitizedEvents: RawEvent[] = [];
    const validationErrors: string[] = [];
    
    for (const event of events) {
      const errors = await this.validator.validateEvent(event);
      if (errors.length > 0) {
        validationErrors.push(`Event ${event.id}: ${errors.join(", ")}`);
        continue;
      }
      
      const sanitized = await this.validator.sanitizeEvent(event);
      sanitizedEvents.push(sanitized);
    }
    
    if (validationErrors.length > 0) {
      throw new Error(`Event validation failed: ${validationErrors.join("; ")}`);
    }
    
    // Sort events by ledger to maintain order
    sanitizedEvents.sort((a, b) => a.ledger - b.ledger);
    
    // Write events to file
    const lines = sanitizedEvents.map(event => JSON.stringify(event));
    await fs.appendFile(this.eventsFile, lines.join("\n") + "\n", "utf8");
    
    // Rotate file if it gets too large
    await this.rotateFileIfNeeded();
  }

  async getEventsByLedgerRange(
    fromLedger: number, 
    toLedger: number, 
    limit?: number
  ): Promise<RawEvent[]> {
    await fs.mkdir(this.dataDir, { recursive: true });
    
    try {
      const data = await fs.readFile(this.eventsFile, "utf8");
      const lines = data.trim().split("\n").filter(line => line.length > 0);
      
      const events: RawEvent[] = [];
      let count = 0;
      
      for (const line of lines) {
        if (limit && count >= limit) break;
        
        try {
          const event = JSON.parse(line) as RawEvent;
          if (event.ledger >= fromLedger && event.ledger <= toLedger) {
            events.push(event);
            count++;
          }
        } catch (parseError) {
          // Skip malformed lines but log them
          console.warn(`Skipping malformed event line: ${line.substring(0, 100)}...`);
        }
      }
      
      return events.sort((a, b) => a.ledger - b.ledger);
    } catch (error) {
      if ((error as any).code === "ENOENT") {
        return [];
      }
      throw error;
    }
  }

  async getEventCount(fromLedger: number, toLedger: number): Promise<number> {
    const events = await this.getEventsByLedgerRange(fromLedger, toLedger);
    return events.length;
  }

  async getLedgerBounds(): Promise<{ min: number | null; max: number | null }> {
    await fs.mkdir(this.dataDir, { recursive: true });
    
    try {
      const data = await fs.readFile(this.eventsFile, "utf8");
      const lines = data.trim().split("\n").filter(line => line.length > 0);
      
      let min: number | null = null;
      let max: number | null = null;
      
      for (const line of lines) {
        try {
          const event = JSON.parse(line) as RawEvent;
          if (min === null || event.ledger < min) min = event.ledger;
          if (max === null || event.ledger > max) max = event.ledger;
        } catch (parseError) {
          // Skip malformed lines
        }
      }
      
      return { min, max };
    } catch (error) {
      if ((error as any).code === "ENOENT") {
        return { min: null, max: null };
      }
      throw error;
    }
  }

  async deleteEventsByLedgerRange(fromLedger: number, toLedger: number): Promise<number> {
    await fs.mkdir(this.dataDir, { recursive: true });
    
    try {
      const data = await fs.readFile(this.eventsFile, "utf8");
      const lines = data.trim().split("\n").filter(line => line.length > 0);
      
      const keptLines: string[] = [];
      let deletedCount = 0;
      
      for (const line of lines) {
        try {
          const event = JSON.parse(line) as RawEvent;
          if (event.ledger >= fromLedger && event.ledger <= toLedger) {
            deletedCount++;
          } else {
            keptLines.push(line);
          }
        } catch (parseError) {
          // Keep malformed lines for debugging
          keptLines.push(line);
        }
      }
      
      // Rewrite file with kept events
      await fs.writeFile(this.eventsFile, keptLines.join("\n") + "\n", "utf8");
      
      return deletedCount;
    } catch (error) {
      if ((error as any).code === "ENOENT") {
        return 0;
      }
      throw error;
    }
  }

  async getReplayCursor(): Promise<number | null> {
    try {
      const data = await fs.readFile(this.cursorFile, "utf8");
      const cursor = JSON.parse(data);
      return typeof cursor.ledger === "number" ? cursor.ledger : null;
    } catch (error) {
      if ((error as any).code === "ENOENT") {
        return null;
      }
      throw error;
    }
  }

  async setReplayCursor(ledger: number): Promise<void> {
    await fs.mkdir(this.dataDir, { recursive: true });
    await fs.writeFile(
      this.cursorFile, 
      JSON.stringify({ ledger, updatedAt: new Date().toISOString() }), 
      "utf8"
    );
  }

  private async rotateFileIfNeeded(): Promise<void> {
    try {
      const stats = await fs.stat(this.eventsFile);
      if (stats.size > this.maxFileSize) {
        const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
        const rotatedFile = path.join(this.dataDir, `events-${timestamp}.jsonl`);
        await fs.rename(this.eventsFile, rotatedFile);
      }
    } catch (error) {
      // File might not exist yet
    }
  }

  // Test helper method
  async reset(): Promise<void> {
    try {
      await fs.rm(this.dataDir, { recursive: true, force: true });
    } catch {
      // Ignore errors during reset
    }
  }
}

// In-memory implementation for testing
export class InMemoryRawEventStore implements RawEventStore {
  private events: RawEvent[] = [];
  private cursor: number | null = null;
  private validator: EventValidator;

  constructor(validator: EventValidator) {
    this.validator = validator;
  }

  async storeEvents(events: RawEvent[]): Promise<void> {
    // Validate and sanitize events
    const sanitizedEvents: RawEvent[] = [];
    
    for (const event of events) {
      const errors = await this.validator.validateEvent(event);
      if (errors.length > 0) {
        throw new Error(`Event validation failed for ${event.id}: ${errors.join(", ")}`);
      }
      
      const sanitized = await this.validator.sanitizeEvent(event);
      sanitizedEvents.push(sanitized);
    }
    
    // Sort and add to store
    sanitizedEvents.sort((a, b) => a.ledger - b.ledger);
    this.events.push(...sanitizedEvents);
  }

  async getEventsByLedgerRange(
    fromLedger: number, 
    toLedger: number, 
    limit?: number
  ): Promise<RawEvent[]> {
    const filtered = this.events
      .filter(e => e.ledger >= fromLedger && e.ledger <= toLedger)
      .sort((a, b) => a.ledger - b.ledger);
    
    return limit ? filtered.slice(0, limit) : filtered;
  }

  async getEventCount(fromLedger: number, toLedger: number): Promise<number> {
    return this.events.filter(e => e.ledger >= fromLedger && e.ledger <= toLedger).length;
  }

  async getLedgerBounds(): Promise<{ min: number | null; max: number | null }> {
    if (this.events.length === 0) {
      return { min: null, max: null };
    }
    
    const ledgers = this.events.map(e => e.ledger);
    return { min: Math.min(...ledgers), max: Math.max(...ledgers) };
  }

  async deleteEventsByLedgerRange(fromLedger: number, toLedger: number): Promise<number> {
    const originalLength = this.events.length;
    this.events = this.events.filter(e => e.ledger < fromLedger || e.ledger > toLedger);
    return originalLength - this.events.length;
  }

  async getReplayCursor(): Promise<number | null> {
    return this.cursor;
  }

  async setReplayCursor(ledger: number): Promise<void> {
    this.cursor = ledger;
  }

  // Test helper
  reset(): void {
    this.events = [];
    this.cursor = null;
  }

  // Test helper
  getEvents(): RawEvent[] {
    return [...this.events];
  }
}
