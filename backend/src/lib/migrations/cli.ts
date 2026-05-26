/**
 * Migration CLI — structured database migration workflow.
 *
 * Forward-Only Migration Policy:
 *  - Each migration file contains ONLY an `up` function (forward direction).
 *  - Down migrations (rollbacks) are EXPLICITLY opt-in per-migration via `meta.allow_down`.
 *  - Running down migrations in production requires TWO-PERSON approval:
 *      1. --emergency flag (acknowledges risk)
 *      2. .hotfix-approvals/<version>_<name>.approval file exists
 *
 * Hotfix Protocol for Production Incidents:
 *   Step 1: Identify problematic migration (e.g., v003_add_column has data corruption)
 *   Step 2: Create hotfix approval file with two senior engineer signatures
 *   Step 3: If immediate fix needed, author v004_hotfix_fix with `meta.hotfix = true`
 *   Step 4: Deploy and run: `npm run migrate -- --allow-down --emergency`
 *   Step 5: Document incident in retro issue
 *
 * See backend/docs/migrations.md for complete operational playbook.
 */

import { migrateCommand } from "./policy";

function parseArgs(): Record<string, unknown> {
  const args: Record<string, unknown> = {};
  for (let i = 2; i < process.argv.length; i++) {
    const arg = process.argv[i];
    if (arg.startsWith("--")) {
      const key = arg.slice(2).replace(/-/g, "");
      // Boolean flags default to true
      args[key] = true;
    }
  }
  return args;
}

async function main(): Promise<void> {
  console.log("🚀 QuickLendX Migration Runner\n");

  const args = parseArgs();

  try {
    const result = await migrateCommand(args);
    if (result.success) {
      process.exit(0);
    } else {
      console.error(`\n❌ ${result.message}`);
      process.exit(1);
    }
  } catch (err: any) {
    console.error("Unexpected error:", err.message);
    process.exit(1);
  }
}

main();
