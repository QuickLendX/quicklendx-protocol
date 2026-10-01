import { exportConcurrencyService } from "../services/exportConcurrency";

describe("ExportConcurrencyService", () => {
  beforeEach(() => {
    exportConcurrencyService.reset();
  });

  it("should allow acquiring slots up to the limit", () => {
    const key = "test-key-1";
    expect(exportConcurrencyService.tryAcquire(key)).toBe(true);
    expect(exportConcurrencyService.tryAcquire(key)).toBe(true);
    expect(exportConcurrencyService.tryAcquire(key)).toBe(false);
  });

  it("should release slots correctly", () => {
    const key = "test-key-2";
    exportConcurrencyService.tryAcquire(key);
    exportConcurrencyService.tryAcquire(key);
    expect(exportConcurrencyService.tryAcquire(key)).toBe(false);

    exportConcurrencyService.release(key);
    expect(exportConcurrencyService.tryAcquire(key)).toBe(true);
  });

  it("should track active counts correctly", () => {
    const key = "test-key-3";
    expect(exportConcurrencyService.getActiveCount(key)).toBe(0);
    exportConcurrencyService.tryAcquire(key);
    expect(exportConcurrencyService.getActiveCount(key)).toBe(1);
    exportConcurrencyService.tryAcquire(key);
    expect(exportConcurrencyService.getActiveCount(key)).toBe(2);
    exportConcurrencyService.release(key);
    expect(exportConcurrencyService.getActiveCount(key)).toBe(1);
    exportConcurrencyService.release(key);
    expect(exportConcurrencyService.getActiveCount(key)).toBe(0);
  });

  it("should handle multiple keys independently", () => {
    const key1 = "test-key-4";
    const key2 = "test-key-5";
    
    expect(exportConcurrencyService.tryAcquire(key1)).toBe(true);
    expect(exportConcurrencyService.tryAcquire(key1)).toBe(true);
    expect(exportConcurrencyService.tryAcquire(key1)).toBe(false);
    
    expect(exportConcurrencyService.tryAcquire(key2)).toBe(true);
    expect(exportConcurrencyService.tryAcquire(key2)).toBe(true);
    expect(exportConcurrencyService.tryAcquire(key2)).toBe(false);
  });

  it("should reset all state correctly", () => {
    const key = "test-key-6";
    exportConcurrencyService.tryAcquire(key);
    exportConcurrencyService.tryAcquire(key);
    expect(exportConcurrencyService.getActiveCount(key)).toBe(2);
    
    exportConcurrencyService.reset();
    expect(exportConcurrencyService.getActiveCount(key)).toBe(0);
    expect(exportConcurrencyService.tryAcquire(key)).toBe(true);
  });

  it("should not go below zero on release of an unheld slot", () => {
    const key = "test-key-7";
    exportConcurrencyService.release(key);
    expect(exportConcurrencyService.getActiveCount(key)).toBe(0);
    expect(exportConcurrencyService.tryAcquire(key)).toBe(true);
    expect(exportConcurrencyService.getActiveCount(key)).toBe(1);
  });

  it("should treat unknown keys as having zero active count", () => {
    expect(exportConcurrencyService.getActiveCount("unknown-key")).toBe(0);
  });

  it("should allow re-acquisition after release in a loop", () => {
    const key = "test-key-8";
    for (let i = 0; i < 5; i++) {
      expect(exportConcurrencyService.tryAcquire(key)).toBe(true);
      expect(exportConcurrencyService.tryAcquire(key)).toBe(true);
      expect(exportConcurrencyService.tryAcquire(key)).toBe(false);
      exportConcurrencyService.release(key);
      exportConcurrencyService.release(key);
      expect(exportConcurrencyService.getActiveCount(key)).toBe(0);
    }
  });
});
