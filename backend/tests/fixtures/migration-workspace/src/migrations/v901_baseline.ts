export default {
  version: 901,
  name: "baseline",
  authoredAt: "2026-04-26",
  author: "test-fixture",
  up: (ctx: any) => {
    ctx.db.exec("CREATE TABLE IF NOT EXISTS probe_log (version INTEGER PRIMARY KEY, note TEXT NOT NULL)");
    ctx.db.run("INSERT INTO probe_log (version, note) VALUES (901, 'baseline')");
  },
  down: (ctx: any) => {
    ctx.db.exec("DELETE FROM probe_log WHERE version = 901");
  },
};