import { describe, expect, it, beforeEach, jest } from "@jest/globals";
import { NotificationDedupCache } from "../src/services/notificationDedupCache";

const TTL_MS = 1000;
const MAX = 3;

describe("NotificationDedupCache", () => {
  let cache: NotificationDedupCache;

  beforeEach(() => {
    jest.useFakeTimers();
    cache = new NotificationDedupCache(MAX, TTL_MS);
  });

  afterEach(() => {
    jest.useRealTimers();
  });

  describe("basic add / has", () => {
    it("returns false for unknown key", () => {
      expect(cache.has("a")).toBe(false);
    });

    it("returns true after add", () => {
      cache.add("a");
      expect(cache.has("a")).toBe(true);
    });

    it("returns false after delete", () => {
      cache.add("a");
      cache.delete("a");
      expect(cache.has("a")).toBe(false);
    });

    it("tracks size correctly", () => {
      expect(cache.size).toBe(0);
      cache.add("a");
      expect(cache.size).toBe(1);
      cache.add("b");
      expect(cache.size).toBe(2);
      cache.delete("a");
      expect(cache.size).toBe(1);
    });
  });

  describe("TTL expiry", () => {
    it("expires entry after TTL", () => {
      cache.add("a");
      jest.advanceTimersByTime(TTL_MS);
      // Exactly at TTL: expiresAt == now, not yet expired
      expect(cache.has("a")).toBe(true);
      jest.advanceTimersByTime(1);
      expect(cache.has("a")).toBe(false);
    });

    it("expired entry does not count toward size", () => {
      cache.add("a");
      jest.advanceTimersByTime(TTL_MS + 1);
      // has() will lazily clean up
      expect(cache.has("a")).toBe(false);
      expect(cache.size).toBe(0);
    });
  });

  describe("max-size eviction (LRU)", () => {
    it("evicts oldest entry when at capacity", () => {
      cache.add("a");
      cache.add("b");
      cache.add("c"); // now full
      cache.add("d"); // should evict "a" (oldest)

      expect(cache.has("a")).toBe(false);
      expect(cache.has("b")).toBe(true);
      expect(cache.has("c")).toBe(true);
      expect(cache.has("d")).toBe(true);
      expect(cache.size).toBe(3);
    });

    it("eviction counter increments only on real evictions", () => {
      expect(cache.evictions).toBe(0);

      cache.add("a");
      cache.add("b");
      cache.add("c");
      expect(cache.evictions).toBe(0); // no eviction yet

      cache.add("d"); // evicts "a"
      expect(cache.evictions).toBe(1);

      cache.add("e"); // evicts "b"
      expect(cache.evictions).toBe(2);
    });

    it("eviction counter does NOT increment on lookups or TTL expiry", () => {
      cache.add("a");
      cache.add("b");
      cache.add("c");
      cache.add("d"); // evicts "a" → 1 eviction

      // Lookup of existing key
      cache.has("b");
      expect(cache.evictions).toBe(1);

      // Lookup of evicted key
      cache.has("a");
      expect(cache.evictions).toBe(1);

      // TTL expiry (no eviction)
      jest.advanceTimersByTime(TTL_MS + 1);
      cache.has("b");
      expect(cache.evictions).toBe(1);
    });

    it("promotes accessed entry (LRU ordering)", () => {
      cache.add("a");
      cache.add("b");
      cache.add("c");

      // access "a" → promotes it to MRU
      cache.has("a");

      cache.add("d"); // should evict "b" (now oldest), not "a"

      expect(cache.has("b")).toBe(false);
      expect(cache.has("a")).toBe(true);
      expect(cache.has("c")).toBe(true);
      expect(cache.has("d")).toBe(true);
    });
  });

  describe("re-add after eviction", () => {
    it("allows re-adding an evicted key", () => {
      cache.add("a");
      cache.add("b");
      cache.add("c");
      cache.add("d"); // evicts "a"

      expect(cache.has("a")).toBe(false);

      cache.add("a"); // re-add after eviction
      expect(cache.has("a")).toBe(true);
      expect(cache.size).toBe(3);
    });

    it("re-add after TTL expiry works", () => {
      cache.add("a");
      jest.advanceTimersByTime(TTL_MS + 1);
      expect(cache.has("a")).toBe(false);

      cache.add("a");
      expect(cache.has("a")).toBe(true);
    });
  });

  describe("concurrent enqueues (rapid sequential adds)", () => {
    it("handles rapid sequential adds without error", () => {
      const keys = Array.from({ length: 100 }, (_, i) => `key-${i}`);
      for (const key of keys) {
        cache.add(key);
      }
      // Only MAX entries survive
      expect(cache.size).toBe(MAX);
      // The last MAX keys exist
      for (let i = 100 - MAX; i < 100; i++) {
        expect(cache.has(`key-${i}`)).toBe(true);
      }
      // First keys were evicted
      expect(cache.has("key-0")).toBe(false);
    });

    it("deduplicates identical keys added rapidly", () => {
      for (let i = 0; i < 10; i++) {
        cache.add("a");
      }
      expect(cache.size).toBe(1);
      expect(cache.has("a")).toBe(true);
    });
  });
});
