interface CacheEntry {
  expiresAt: number;
}

export class NotificationDedupCache {
  private maxSize: number;
  private ttlMs: number;
  private cache: Map<string, CacheEntry>;
  private evictionCount: number;

  constructor(maxSize: number, ttlMs: number) {
    this.maxSize = maxSize;
    this.ttlMs = ttlMs;
    this.cache = new Map();
    this.evictionCount = 0;
  }

  has(key: string): boolean {
    const entry = this.cache.get(key);
    if (!entry) return false;
    if (Date.now() > entry.expiresAt) {
      this.cache.delete(key);
      return false;
    }
    // LRU promote: delete + re-insert moves key to end (most recently used)
    this.cache.delete(key);
    this.cache.set(key, entry);
    return true;
  }

  add(key: string): void {
    // Remove existing entry so re-insert moves it to end
    this.cache.delete(key);

    // Evict oldest entries if at capacity
    while (this.cache.size >= this.maxSize) {
      const oldestKey = this.cache.keys().next().value;
      if (oldestKey !== undefined) {
        this.cache.delete(oldestKey);
        this.evictionCount++;
      } else {
        break;
      }
    }

    this.cache.set(key, { expiresAt: Date.now() + this.ttlMs });
  }

  delete(key: string): void {
    this.cache.delete(key);
  }

  get size(): number {
    return this.cache.size;
  }

  get evictions(): number {
    return this.evictionCount;
  }
}
