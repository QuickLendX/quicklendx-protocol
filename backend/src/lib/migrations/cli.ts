/**
 * Migration CLI — structured database migration workflow.
 *
 * Migration Commands:
 *  - npm run migrate: Run pending migrations (up)
 *  - npm run migrate:down: Rollback the last applied migration
 *  - npm run migrate:down -- --to <version>: Rollback to specific version
 *  - npm run migrate:down -- --all: Rollback all migrations
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
 *   Step 4: Deploy and run: `npm run migrate:down -- --emergency`
 *   Step 5: Document incident in retro issue
 *
 * See backend/docs/migrations.md for complete operational playbook.
 */

import { migrateCommand, migrateDownCommand } from "./policy";

/** Custom error types for deterministic argument parsing. */
export class InvalidArgumentError extends Error {
  constructor(arg: string) {
    super(`Invalid argument: ${arg}`);
    this.name = "InvalidArgumentError";
  }
}
export class PermissionError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "PermissionError";
  }
}

/**
 * Parse command‑line arguments with deterministic validation.
 *
 * - Flags start with `--` and are converted to camelCase keys.
 * - Boolean flags without a value default to `true`.
 * - Flags can accept a following non‑flag token as a value.
 * - Positional arguments are collected in the `_` array.
 * - Invalid syntax (single dash, lone `--`) throws `InvalidArgumentError`.
 * - Permission‑sensitive flags (e.g., `--emergency`) throw `PermissionError`
 *   when the runtime lacks appropriate privileges.
 */
export function parseArgs(argv: string[] = process.argv): Record<string, unknown> {
  const args: Record<string, unknown> = {};
  for (let i = 2; i < argv.length; i++) {
    const arg = argv[i];

    // Detect invalid short flags or lone '--'
    if (arg.startsWith("-") && !arg.startsWith("--")) {
      throw new InvalidArgumentError(arg);
    }
    if (arg === "--") {
      throw new InvalidArgumentError(arg);
    }

    if (arg.startsWith("--")) {
      const keyRaw = arg.slice(2);
      const key = keyRaw.replace(/-([a-z])/g, (_, c) => c.toUpperCase());

      // Look ahead for a value token
      const next = argv[i + 1];
      if (next && !next.startsWith("-")) {
        args[key] = next;
        i++; // consume value
      } else {
        args[key] = true;
      }

      // Example permission check for emergency flag
      if (key === "emergency") {
        if (typeof (process as any).getuid !== "function") {
          throw new PermissionError(
            "Emergency migrations require admin privileges on this platform."
          );
        }
      }
    } else {
      // Positional arguments
      if (!args._) args._ = [];
      (args._ as string[]).push(arg);
    }
  }
  return args;
}

async function main(): Promise<void> {
  console.log("🚀 QuickLendX Migration Runner\n");

  const args = parseArgs();
  const parsedArgs: any = args;
const command = parsedArgs._?.[0] || "up";

  try {
    let result;
    if (command === "down") {
      result = await migrateDownCommand(args);
    } else {
      result = await migrateCommand(args);
    }

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
