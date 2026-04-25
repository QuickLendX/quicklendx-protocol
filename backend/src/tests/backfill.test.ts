import request from "supertest";
import app from "../index";
import { backfillService } from "../services/backfillService";

const ADMIN_TOKEN = "test-admin-token";

describe("Admin backfill tooling", () => {
  beforeEach(async () => {
    process.env.ADMIN_API_TOKEN = ADMIN_TOKEN;
    process.env.BACKFILL_MAX_LEDGER_RANGE = "100";
    process.env.BACKFILL_MAX_CONCURRENCY = "3";
    await backfillService.resetForTests();
  });

  it("requires admin auth", async () => {
    const res = await request(app).post("/api/admin/backfill").send({
      startLedger: 1,
      endLedger: 10,
      dryRun: true,
    });
    expect(res.status).toBe(401);
  });

  it("returns dry-run preview without creating a run", async () => {
    const res = await request(app)
      .post("/api/admin/backfill")
      .set("Authorization", `Bearer ${ADMIN_TOKEN}`)
      .set("x-admin-actor", "ops-user")
      .send({
        startLedger: 5,
        endLedger: 10,
        dryRun: true,
        concurrency: 2,
      });

    expect(res.status).toBe(200);
    expect(res.body.preview.range.totalLedgers).toBe(6);
    expect(res.body.run).toBeUndefined();
  });

  it("enforces max range guardrail", async () => {
    const res = await request(app)
      .post("/api/admin/backfill")
      .set("Authorization", `Bearer ${ADMIN_TOKEN}`)
      .send({
        startLedger: 1,
        endLedger: 200,
        dryRun: false,
        concurrency: 1,
      });

    expect(res.status).toBe(422);
    expect(res.body.code).toBe("MAX_RANGE_EXCEEDED");
  });

  it("enforces max concurrency guardrail", async () => {
    const res = await request(app)
      .post("/api/admin/backfill")
      .set("Authorization", `Bearer ${ADMIN_TOKEN}`)
      .send({
        startLedger: 1,
        endLedger: 10,
        dryRun: false,
        concurrency: 9,
      });

    expect(res.status).toBe(422);
    expect(res.body.code).toBe("MAX_CONCURRENCY_EXCEEDED");
  });

  it("supports pause and resume", async () => {
    const startRes = await request(app)
      .post("/api/admin/backfill")
      .set("Authorization", `Bearer ${ADMIN_TOKEN}`)
      .send({
        startLedger: 1,
        endLedger: 80,
        dryRun: false,
        concurrency: 1,
      });

    expect(startRes.status).toBe(202);
    const runId = startRes.body.run.id as string;

    const pauseRes = await request(app)
      .post("/api/admin/backfill/pause")
      .set("Authorization", `Bearer ${ADMIN_TOKEN}`)
      .send({ runId });
    expect([200, 409]).toContain(pauseRes.status);

    const runAfterPause = await request(app)
      .get(`/api/admin/backfill/${runId}`)
      .set("Authorization", `Bearer ${ADMIN_TOKEN}`);
    expect(runAfterPause.status).toBe(200);

    if (runAfterPause.body.run.status === "paused") {
      const resumeRes = await request(app)
        .post("/api/admin/backfill/resume")
        .set("Authorization", `Bearer ${ADMIN_TOKEN}`)
        .send({ runId });
      expect(resumeRes.status).toBe(200);
      expect(resumeRes.body.run.status).toBe("running");
    }
  });

  it("supports idempotency reuse", async () => {
    const payload = {
      startLedger: 10,
      endLedger: 20,
      dryRun: false,
      concurrency: 1,
      idempotencyKey: "bf-job-1",
    };

    const first = await request(app)
      .post("/api/admin/backfill")
      .set("Authorization", `Bearer ${ADMIN_TOKEN}`)
      .send({ ...payload });
    const second = await request(app)
      .post("/api/admin/backfill")
      .set("Authorization", `Bearer ${ADMIN_TOKEN}`)
      .send({ ...payload });

    expect(first.status).toBe(202);
    expect(second.status).toBe(202);
    expect(second.body.idempotentReuse).toBe(true);
    expect(second.body.run.id).toBe(first.body.run.id);
  });

  it("handles partial failure and allows resume from failed cursor", async () => {
    backfillService.setFailureAtLedgerForTests(20);
    const startRes = await request(app)
      .post("/api/admin/backfill")
      .set("Authorization", `Bearer ${ADMIN_TOKEN}`)
      .send({
        startLedger: 1,
        endLedger: 80,
        dryRun: false,
        concurrency: 1,
      });
    expect(startRes.status).toBe(202);
    const runId = startRes.body.run.id as string;

    // Allow async runner ticks to execute.
    await new Promise((resolve) => setTimeout(resolve, 20));

    const failed = await request(app)
      .get(`/api/admin/backfill/${runId}`)
      .set("Authorization", `Bearer ${ADMIN_TOKEN}`);
    expect(failed.status).toBe(200);
    expect(failed.body.run.status).toBe("failed");

    backfillService.setFailureAtLedgerForTests(null);

    const resumeRes = await request(app)
      .post("/api/admin/backfill/resume")
      .set("Authorization", `Bearer ${ADMIN_TOKEN}`)
      .send({ runId });
    expect(resumeRes.status).toBe(200);
  });
});
