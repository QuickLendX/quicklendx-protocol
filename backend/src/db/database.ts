/**
 * Simple in-memory database for API keys and audit logs
 * In production, replace with PostgreSQL, MySQL, or another persistent database
 */

export interface DbApiKey {
  id: string;
  key_hash: string;
  prefix: string;
  name: string;
  scopes: string;
  created_at: string;
  last_used_at: string | null;
  expires_at: string | null;
  revoked: number;
  created_by: string;
}

export interface DbAuditLog {
  id: string;
  event_type: 'created' | 'used' | 'rotated' | 'revoked';
  key_id: string;
  actor: string;
  timestamp: string;
  ip_address: string | null;
  endpoint: string | null;
  metadata: string | null;
}

class Database {
  private apiKeys: Map<string, DbApiKey> = new Map();
  private auditLogs: DbAuditLog[] = [];
  private keysByPrefix: Map<string, string> = new Map();

  constructor() {
    this.initialize();
  }

  private initialize() {
    // Initialize in-memory storage
    // In production, this would connect to a real database
  }

  // API Key operations
  createApiKey(key: DbApiKey): void {
    this.apiKeys.set(key.id, key);
    this.keysByPrefix.set(key.prefix, key.id);
  }

  getApiKeyById(id: string): DbApiKey | undefined {
    return this.apiKeys.get(id);
  }

  getApiKeyByPrefix(prefix: string): DbApiKey | undefined {
    const id = this.keysByPrefix.get(prefix);
    return id ? this.apiKeys.get(id) : undefined;
  }

  updateApiKey(id: string, updates: Partial<DbApiKey>): boolean {
    const key = this.apiKeys.get(id);
    if (!key) return false;

    const updated = { ...key, ...updates };
    this.apiKeys.set(id, updated);
    return true;
  }

  deleteApiKey(id: string): boolean {
    const key = this.apiKeys.get(id);
    if (!key) return false;

    this.keysByPrefix.delete(key.prefix);
    return this.apiKeys.delete(id);
  }

  listApiKeys(filters?: { created_by?: string; revoked?: boolean }): DbApiKey[] {
    let keys = Array.from(this.apiKeys.values());

    if (filters?.created_by) {
      keys = keys.filter(k => k.created_by === filters.created_by);
    }

    if (filters?.revoked !== undefined) {
      keys = keys.filter(k => (k.revoked === 1) === filters.revoked);
    }

    return keys;
  }

  // Audit log operations
  createAuditLog(log: DbAuditLog): void {
    this.auditLogs.push(log);
  }

  getAuditLogs(filters?: { key_id?: string; event_type?: string }): DbAuditLog[] {
    let logs = [...this.auditLogs];

    if (filters?.key_id) {
      logs = logs.filter(l => l.key_id === filters.key_id);
    }

    if (filters?.event_type) {
      logs = logs.filter(l => l.event_type === filters.event_type);
    }

    return logs.sort((a, b) => 
      new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime()
    );
  }

  // Utility methods for testing
  clear(): void {
    this.apiKeys.clear();
    this.auditLogs = [];
    this.keysByPrefix.clear();
  }

  getStats() {
    return {
      apiKeys: this.apiKeys.size,
      auditLogs: this.auditLogs.length,
    };
  }
}

// Singleton instance
export const db = new Database();
