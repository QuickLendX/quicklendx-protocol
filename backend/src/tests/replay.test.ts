import { ReplayService } from "../services/replayService";
import { DefaultEventValidator } from "../services/eventValidator";
import { InMemoryRawEventStore } from "../services/rawEventStore";
import { InMemoryDerivedTableStore } from "../services/derivedTableStore";
import { RawEvent, ReplayStartRequest } from "../types/replay";

describe("ReplayService", () => {
  let replayService: ReplayService;
  let rawEventStore: InMemoryRawEventStore;
  let derivedTableStore: InMemoryDerivedTableStore;
  let eventValidator: DefaultEventValidator;

  beforeEach(() => {
    eventValidator = new DefaultEventValidator();
    rawEventStore = new InMemoryRawEventStore(eventValidator);
    derivedTableStore = new InMemoryDerivedTableStore();
    replayService = ReplayService.getInstance(rawEventStore, derivedTableStore);
  });

  afterEach(async () => {
    await replayService.resetForTests();
    rawEventStore.reset();
    derivedTableStore.reset();
  });

  describe("Replay Determinism", () => {
    test("replay twice yields same DB state", async () => {
      // Setup: Create test events
      const testEvents: RawEvent[] = [
        {
          id: "event_1",
          ledger: 1000,
          txHash: "0x1234567890abcdef",
          type: "InvoiceCreated",
          payload: {
            invoice_id: "inv_001",
            business: "business_1",
            amount: "1000",
            currency: "USD"
          },
          timestamp: Date.now(),
          complianceHold: false,
          indexedAt: new Date().toISOString()
        },
        {
          id: "event_2", 
          ledger: 1001,
          txHash: "0x1234567890abcdef",
          type: "InvoiceSettled",
          payload: {
            invoice_id: "inv_001",
            business: "business_1",
            investor: "investor_1",
            amount: "1000"
          },
          timestamp: Date.now() + 1000,
          complianceHold: false,
          indexedAt: new Date().toISOString()
        },
        {
          id: "event_3",
          ledger: 1002,
          txHash: "0x1234567890abcdef", 
          type: "PaymentRecorded",
          payload: {
            invoice_id: "inv_001",
            payer: "business_1",
            amount: "500"
          },
          timestamp: Date.now() + 2000,
          complianceHold: false,
          indexedAt: new Date().toISOString()
        }
      ];

      // Store events
      await rawEventStore.storeEvents(testEvents);

      // First replay
      const firstReplayRequest: ReplayStartRequest = {
        fromLedger: 1000,
        toLedger: 1002,
        dryRun: false,
        batchSize: 2,
        forceRebuild: true
      };

      const firstResult = await replayService.startReplay(firstReplayRequest, "test_actor");
      expect(firstResult.run).toBeDefined();
      expect(firstResult.run!.status).toBe("pending");

      // Wait for first replay to complete
      await waitForCompletion(replayService, firstResult.run!.id);

      // Get state after first replay
      const firstStateHash = await derivedTableStore.getStateHash();
      const firstTableCounts = derivedTableStore.getTableCounts();

      // Second replay (same range, same events)
      const secondReplayRequest: ReplayStartRequest = {
        fromLedger: 1000,
        toLedger: 1002,
        dryRun: false,
        batchSize: 2,
        forceRebuild: true
      };

      const secondResult = await replayService.startReplay(secondReplayRequest, "test_actor");
      expect(secondResult.run).toBeDefined();

      // Wait for second replay to complete
      await waitForCompletion(replayService, secondResult.run!.id);

      // Get state after second replay
      const secondStateHash = await derivedTableStore.getStateHash();
      const secondTableCounts = derivedTableStore.getTableCounts();

      // Verify deterministic results
      expect(firstStateHash).toBe(secondStateHash);
      expect(firstTableCounts).toEqual(secondTableCounts);
      
      // Verify specific table contents
      expect(firstTableCounts.notifications).toBeGreaterThan(0);
      expect(firstTableCounts.invoices).toBe(0); // Events don't create invoices directly
    });

    test("replay with different batch sizes yields same result", async () => {
      const testEvents: RawEvent[] = [
        {
          id: "event_batch_1",
          ledger: 2000,
          txHash: "0xabcdef1234567890",
          type: "BidPlaced",
          payload: {
            bid_id: "bid_001",
            invoice_id: "inv_002",
            investor: "investor_2",
            bid_amount: "1500"
          },
          timestamp: Date.now(),
          complianceHold: false,
          indexedAt: new Date().toISOString()
        },
        {
          id: "event_batch_2",
          ledger: 2001,
          txHash: "0xabcdef1234567890",
          type: "BidAccepted",
          payload: {
            bid_id: "bid_001",
            invoice_id: "inv_002"
          },
          timestamp: Date.now() + 1000,
          complianceHold: false,
          indexedAt: new Date().toISOString()
        }
      ];

      await rawEventStore.storeEvents(testEvents);

      // First replay with batch size 1
      const firstReplay = await replayService.startReplay({
        fromLedger: 2000,
        toLedger: 2001,
        dryRun: false,
        batchSize: 1,
        forceRebuild: true
      }, "test_actor");

      await waitForCompletion(replayService, firstReplay.run!.id);
      const firstStateHash = await derivedTableStore.getStateHash();

      // Reset and replay with batch size 2
      await derivedTableStore.reset();
      const secondReplay = await replayService.startReplay({
        fromLedger: 2000,
        toLedger: 2001,
        dryRun: false,
        batchSize: 2,
        forceRebuild: true
      }, "test_actor");

      await waitForCompletion(replayService, secondReplay.run!.id);
      const secondStateHash = await derivedTableStore.getStateHash();

      expect(firstStateHash).toBe(secondStateHash);
    });
  });

  describe("Idempotency", () => {
    test("idempotency key prevents duplicate runs", async () => {
      const testEvents: RawEvent[] = [
        {
          id: "idempotent_event",
          ledger: 3000,
          txHash: "0xdeadbeef12345678",
          type: "DisputeCreated",
          payload: {
            invoice_id: "inv_003",
            initiator: "business_2",
            reason: "Payment dispute"
          },
          timestamp: Date.now(),
          complianceHold: false,
          indexedAt: new Date().toISOString()
        }
      ];

      await rawEventStore.storeEvents(testEvents);

      const request: ReplayStartRequest = {
        fromLedger: 3000,
        toLedger: 3000,
        dryRun: false,
        batchSize: 1,
        forceRebuild: true,
        idempotencyKey: "unique_key_123"
      };

      // First request
      const firstResult = await replayService.startReplay(request, "test_actor");
      expect(firstResult.idempotentReuse).toBeUndefined();
      expect(firstResult.run).toBeDefined();

      // Second request with same idempotency key
      const secondResult = await replayService.startReplay(request, "test_actor");
      expect(secondResult.idempotentReuse).toBe(true);
      expect(secondResult.run?.id).toBe(firstResult.run?.id);
    });

    test("replay cursor consistency", async () => {
      const testEvents: RawEvent[] = [
        {
          id: "cursor_event_1",
          ledger: 4000,
          txHash: "0xcafebabe12345678",
          type: "InvoiceCreated",
          payload: { invoice_id: "inv_004", business: "business_3" },
          timestamp: Date.now(),
          complianceHold: false,
          indexedAt: new Date().toISOString()
        },
        {
          id: "cursor_event_2",
          ledger: 4001,
          txHash: "0xcafebabe12345678",
          type: "InvoiceSettled",
          payload: { invoice_id: "inv_004", business: "business_3", investor: "investor_3" },
          timestamp: Date.now() + 1000,
          complianceHold: false,
          indexedAt: new Date().toISOString()
        }
      ];

      await rawEventStore.storeEvents(testEvents);

      // First partial replay
      const firstReplay = await replayService.startReplay({
        fromLedger: 4000,
        toLedger: 4000,
        dryRun: false,
        batchSize: 1,
        forceRebuild: true
      }, "test_actor");

      await waitForCompletion(replayService, firstReplay.run!.id);
      const cursorAfterFirst = await rawEventStore.getReplayCursor();
      expect(cursorAfterFirst).toBe(4001);

      // Second replay from cursor
      const secondReplay = await replayService.startReplay({
        fromLedger: 4001,
        toLedger: 4001,
        dryRun: false,
        batchSize: 1,
        forceRebuild: false
      }, "test_actor");

      await waitForCompletion(replayService, secondReplay.run!.id);
      const cursorAfterSecond = await rawEventStore.getReplayCursor();
      expect(cursorAfterSecond).toBe(4002);
    });
  });

  describe("Error Handling", () => {
    test("invalid ledger range throws error", async () => {
      const request: ReplayStartRequest = {
        fromLedger: 5000,
        toLedger: 4999, // Invalid: toLedger < fromLedger
        dryRun: false,
        batchSize: 1,
        forceRebuild: true
      };

      await expect(
        replayService.startReplay(request, "test_actor")
      ).rejects.toThrow("INVALID_LEDGER_RANGE");
    });

    test("exceeding max range throws error", async () => {
      const request: ReplayStartRequest = {
        fromLedger: 0,
        toLedger: 200000, // Exceeds default max of 100k
        dryRun: false,
        batchSize: 1,
        forceRebuild: true
      };

      await expect(
        replayService.startReplay(request, "test_actor")
      ).rejects.toThrow("MAX_RANGE_EXCEEDED");
    });

    test("pause and resume functionality", async () => {
      const testEvents: RawEvent[] = [
        {
          id: "pause_event_1",
          ledger: 6000,
          txHash: "0xfeedface12345678",
          type: "InvoiceCreated",
          payload: { invoice_id: "inv_005", business: "business_4" },
          timestamp: Date.now(),
          complianceHold: false,
          indexedAt: new Date().toISOString()
        },
        {
          id: "pause_event_2",
          ledger: 6001,
          txHash: "0xfeedface12345678",
          type: "InvoiceSettled",
          payload: { invoice_id: "inv_005", business: "business_4" },
          timestamp: Date.now() + 1000,
          complianceHold: false,
          indexedAt: new Date().toISOString()
        }
      ];

      await rawEventStore.storeEvents(testEvents);

      // Start replay
      const replay = await replayService.startReplay({
        fromLedger: 6000,
        toLedger: 6001,
        dryRun: false,
        batchSize: 1,
        forceRebuild: true
      }, "test_actor");

      // Pause after a short delay
      await new Promise(resolve => setTimeout(resolve, 10));
      const pausedRun = await replayService.pauseRun(replay.run!.id, "test_actor");
      expect(pausedRun.status).toBe("paused");

      // Resume
      const resumedRun = await replayService.resumeRun(replay.run!.id, "test_actor");
      expect(resumedRun.status).toBe("running");

      // Wait for completion
      await waitForCompletion(replayService, replay.run!.id);
      const finalRun = replayService.getRun(replay.run!.id);
      expect(finalRun?.status).toBe("completed");
    });
  });

  describe("Security Validation", () => {
    test("rejects events with malicious content", async () => {
      const maliciousEvent: RawEvent = {
        id: "malicious_event",
        ledger: 7000,
        txHash: "0xbadcontent12345678",
        type: "InvoiceCreated",
        payload: {
          invoice_id: "inv_006",
          business: "<script>alert('xss')</script>",
          malicious_field: "javascript:alert('xss')"
        },
        timestamp: Date.now(),
        complianceHold: false,
        indexedAt: new Date().toISOString()
      };

      // Should throw validation error
      await expect(
        rawEventStore.storeEvents([maliciousEvent])
      ).rejects.toThrow("Event validation failed");
    });

    test("sanitizes event content", async () => {
      const eventWithNullBytes: RawEvent = {
        id: "sanitize_event\x00\x01\x02",
        ledger: 8000,
        txHash: "0xsanitize12345678\x00",
        type: "InvoiceCreated",
        payload: {
          invoice_id: "inv_007\x00",
          business: "business_5"
        },
        timestamp: Date.now(),
        complianceHold: false,
        indexedAt: new Date().toISOString()
      };

      const sanitizedEvent = await eventValidator.sanitizeEvent(eventWithNullBytes);
      
      // Should remove null bytes and control characters
      expect(sanitizedEvent.id).toBe("sanitize_event");
      expect(sanitizedEvent.txHash).toBe("0xsanitize12345678");
      expect(sanitizedEvent.payload.invoice_id).toBe("inv_007");
    });
  });
});

// Helper function to wait for replay completion
async function waitForCompletion(
  replayService: ReplayService,
  runId: string,
  timeoutMs: number = 10000
): Promise<void> {
  const startTime = Date.now();
  
  while (Date.now() - startTime < timeoutMs) {
    const run = replayService.getRun(runId);
    if (!run) {
      throw new Error(`Run ${runId} not found`);
    }
    
    if (run.status === "completed") {
      return;
    }
    
    if (run.status === "failed") {
      throw new Error(`Replay failed: ${run.error}`);
    }
    
    await new Promise(resolve => setTimeout(resolve, 10));
  }
  
  throw new Error(`Replay timeout after ${timeoutMs}ms`);
}
