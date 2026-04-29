import Database from 'better-sqlite3';
import { config } from '../config';

// Type declaration for better-sqlite3
const DatabaseConstructor = Database as any;

let dbInstance: any = null;

/**
 * Get a singleton instance of the better-sqlite3 database.
 */
export function getDatabase() {
  if (!dbInstance) {
    const db = new DatabaseConstructor(process.env.DATABASE_PATH || '.data/dev.db');
    db.pragma('journal_mode = WAL');
    db.pragma('foreign_keys = ON');
    db.pragma('busy_timeout = 5000');
    dbInstance = db;
  }
  return dbInstance;
}

export function closeDatabase() {
  if (dbInstance) {
    dbInstance.close();
    dbInstance = null;
  }
}