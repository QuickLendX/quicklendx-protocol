export default {
  version: 903,
  name: "hotfix_record",
  authoredAt: "2026-04-26",
  author: "test-fixture",
  meta: {
    hotfix: true,
    reason: "test fixture hotfix",
    rollback_risk: "low",
  },
  up: (ctx: any) => {
    ctx.db.run("INSERT INTO probe_log (version, note) VALUES (903, 'hotfix')");
  },
  down: (ctx: any) => {
    ctx.db.exec("DELETE FROM probe_log WHERE version = 903");
  },
};