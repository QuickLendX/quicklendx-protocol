import { DefaultEventValidator } from "../services/eventValidator";
import { InMemoryRawEventStore } from "../services/rawEventStore";
import { InMemoryDerivedTableStore } from "../services/derivedTableStore";
import { ReplayService } from "../services/replayService";
import { RawEvent, ReplayStartRequest } from "../types/replay";

async function verifyReplayDeterminism(): Promise<void> {
  console.log("🔍 Starting replay determinism verification...");

  // Setup
  const eventValidator = new DefaultEventValidator();
  const rawEventStore = new InMemoryRawEventStore(eventValidator);
  const derivedTableStore = new InMemoryDerivedTableStore();
  const replayService = ReplayService.getInstance(rawEventStore, derivedTableStore);

  // Create test events
  const testEvents: RawEvent[] = [
    {
      id: "verify_event_1",
      ledger: 1000,
      txHash: "0x1234567890abcdef",
      type: "InvoiceCreated",
      payload: {
        invoice_id: "inv_verify_001",
        business: "business_verify_1",
        amount: "1000",
        currency: "USD"
      },
      timestamp: Date.now(),
      complianceHold: false,
      indexedAt: new Date().toISOString()
    },
    {
      id: "verify_event_2",
      ledger: 1001,
      txHash: "0x1234567890abcdef",
      type: "InvoiceSettled",
      payload: {
        invoice_id: "inv_verify_001",
        business: "business_verify_1",
        investor: "investor_verify_1",
        amount: "1000"
      },
      timestamp: Date.now() + 1000,
      complianceHold: false,
      indexedAt: new Date().toISOString()
    }
  ];

  try {
    // Store events
    await rawEventStore.storeEvents(testEvents);
    console.log("✅ Test events stored successfully");

    // First replay
    const firstRequest: ReplayStartRequest = {
      fromLedger: 1000,
      toLedger: 1001,
      dryRun: false,
      batchSize: 1,
      forceRebuild: true
    };

    const firstResult = await replayService.startReplay(firstRequest, "verification_actor");
    if (!firstResult.run) {
      throw new Error("First replay failed to start");
    }
    console.log("✅ First replay started");

    // Wait for completion
    await waitForCompletion(replayService, firstResult.run.id);
    const firstStateHash = await derivedTableStore.getStateHash();
    const firstCounts = derivedTableStore.getTableCounts();
    console.log(`✅ First replay completed. State hash: ${firstStateHash.substring(0, 16)}...`);
    console.log(`   Table counts: ${JSON.stringify(firstCounts)}`);

    // Reset for second replay
    await derivedTableStore.reset();
    console.log("✅ Derived tables reset");

    // Second replay
    const secondResult = await replayService.startReplay(firstRequest, "verification_actor");
    if (!secondResult.run) {
      throw new Error("Second replay failed to start");
    }
    console.log("✅ Second replay started");

    await waitForCompletion(replayService, secondResult.run.id);
    const secondStateHash = await derivedTableStore.getStateHash();
    const secondCounts = derivedTableStore.getTableCounts();
    console.log(`✅ Second replay completed. State hash: ${secondStateHash.substring(0, 16)}...`);
    console.log(`   Table counts: ${JSON.stringify(secondCounts)}`);

    // Verify determinism
    if (firstStateHash === secondStateHash) {
      console.log("🎉 SUCCESS: Replay is deterministic! State hashes match.");
    } else {
      console.log("❌ FAILURE: Replay is not deterministic! State hashes differ.");
      console.log(`   First hash:  ${firstStateHash}`);
      console.log(`   Second hash: ${secondStateHash}`);
    }

    // Verify table counts match
    if (JSON.stringify(firstCounts) === JSON.stringify(secondCounts)) {
      console.log("🎉 SUCCESS: Table counts match between replays!");
    } else {
      console.log("❌ FAILURE: Table counts differ between replays!");
      console.log(`   First counts:  ${JSON.stringify(firstCounts)}`);
      console.log(`   Second counts: ${JSON.stringify(secondCounts)}`);
    }

    // Test idempotency
    const idempotentResult = await replayService.startReplay({
      ...firstRequest,
      idempotencyKey: "test_key_123"
    }, "verification_actor");

    if (idempotentResult.idempotentReuse) {
      console.log("✅ SUCCESS: Idempotency key prevents duplicate runs!");
    } else {
      console.log("❌ FAILURE: Idempotency key not working!");
    }

    console.log("🔍 Verification completed!");

  } catch (error) {
    console.error("❌ Verification failed:", error);
    throw error;
  } finally {
    // Cleanup
    await replayService.resetForTests();
    rawEventStore.reset();
    derivedTableStore.reset();
  }
}

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
    
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  
  throw new Error(`Replay timeout after ${timeoutMs}ms`);
}

// Run verification if this file is executed directly
if (require.main === module) {
  verifyReplayDeterminism()
    .then(() => {
      console.log("🎉 All verifications passed!");
      process.exit(0);
    })
    .catch((error) => {
      console.error("💥 Verification failed:", error);
      process.exit(1);
    });
}

export { verifyReplayDeterminism };
