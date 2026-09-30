export default {
  version: 906,
  name: "dup_alpha",
  authoredAt: "2026-04-26",
  author: "test-fixture",
  up: (ctx: any) => {
    ctx.db.exec("CREATE TABLE IF NOT EXISTS probe_log (version INTEGER PRIMARY KEY, note TEXT NOT NULL)");
    ctx.db.run("INSERT INTO probe_log (version, note) VALUES (906, 'alpha')");
  },
  down: (ctx: any) => {
    ctx.db.exec("DELETE FROM probe_log WHERE version = 906");
  },
};