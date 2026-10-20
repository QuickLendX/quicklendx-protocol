export default {
  version: 905,
  name: "durable",
  authoredAt: "2026-04-26",
  author: "test-fixture",
  up: (ctx: any) => {
    ctx.db.run("INSERT INTO probe_log (version, note) VALUES (905, 'durable')");
  },
  down: (ctx: any) => {
    ctx.db.exec("DELETE FROM probe_log WHERE version = 905");
  },
};