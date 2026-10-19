export default {
  version: 902,
  name: "gated",
  authoredAt: "2026-04-26",
  author: "test-fixture",
  up: (ctx: any) => {
    ctx.db.run("INSERT INTO probe_log (version, note) VALUES (902, 'first')");
    if (process.env.QFC_MIGRATION_902_ALLOWED !== "1") {
      throw new Error("Simulated failure for migration 902");
    }
    ctx.db.run("UPDATE probe_log SET note = 'second' WHERE version = 902");
  },
  down: (ctx: any) => {
    ctx.db.exec("DELETE FROM probe_log WHERE version = 902");
  },
};