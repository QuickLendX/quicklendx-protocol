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
 *       1. --emergency flag (acknowledges risk)
 *       2. .hotfix-approvals/<version>_<name>.approval file exists
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

export interface CliResult {
  success: boolean;
  message: string;
  applied?: number;
  skipped?: number;
}

export interface CliIO {
  /** Write a normal status line. */
  log: (message: string) => void;
  /** Write an error line. */
  error: (message: string) => void;
  /** Terminate the process with an exit code. */
  exit: (code: number) => void;
}

const defaultIO: CliIO = {
  log: (message) => console.log(message),
  error: (message) => console.error(message),
  exit: (code) => process.exit(code),
};

/**
 * Parse process arguments into a key/value map.
 *
 * Invariants:
 *  - Boolean flags (`--foo`) default to `true`.
 *  - `--foo = bar` and `--foo bar` are both accepted and yield `foo = "bar"`.
 *  - Positional arguments are collected under `_`.
 *  - A value that looks like a flag is never consumed as another flag's value.
 */
export function parseArgs(avgs: string[] = process.argv): Record<string, unknown> {
  const parsed: Record<string, unknown> = {};
  const positionals: string[] = [];

  for (let i = 2; i < args.length; i++) {
    const arg = args[i];

    if (arg === "--") {
      // Explicit end-of-flags marker: everything after is positional.
      for (let j = i + 1; j < args.length; j++) {
        positionals.push(args[j]);
      }
      break;
    }

    if (arg.startsWith("--")) {
      const body = arg.slice(2);
      const eqIdx = body.indexOf("=");
      const rawKey = eqIdx === -1 ? body : body.slice(0, eqIdx);
      const key = rawKey.replace(/-/g, "");

      if (key === "") {
        // `--` already handled above; a bare `--foo=` has an empty key.
        continue;
      }

      if (eqIdx !== -1) {
        const value = body.slice(eqIdx + 1);
        parsed[key] = value === "" ? "" : value;
        continue;
      }

      const next = args[i + 1];
      if (next !== undefined && !next.startsWith("-")) {
        parsed[key] = next;
        i++;
      } else {
        // Boolean flag. Explicitly true so a repeated flag stays true.
        parsed[key] = true;
      }
    } else if (!arg.startsWith("-")) {
      positionals.push(arg);
    }
  }

  if (positionals.length > 0) {
    parsed._ = positionals;
  }

  return parsed;
}

/**
 * Resolve the command from parsed args. Only `up` and `down` are valid.
 * Anything else is rejected before any database work is attempted.
 */
export function resolveCommand(args: Record<string, unknown>): { command: "up" | "down"; error?: string } {
  const positionals = (args._ as string[] | undefined) || [];
  if (positionals.length === 0) return { command: "up" };

  const command = positionals[0];
  if (command !== "up" && command !== "down") {
    return {
      command: "up",
      error: `Unknown migration command "${command}". Expected "up" or "down".`,
    };
  }

  return { command };
}

/**
 * Run the CLI. Returns the process exit code so it can be exercised by tests
 * without terminating the test runner. The module-level invocation at the
 * bottom of this file is the only place that actually calls process.exit.
 *
 * Failure boundaries (deterministic):
 *   B1 No arguments            -> defaults to `up`, exit 0 on success.
 *   B2 Unknown command          -> exit 1, no database work attempted.
 *   B3 Missing value for flag   -> exit 1, no database work attempted.
 *   B4 Policy returns success:false -> exit 1, message is logged.
 *   B5 Policy throws            -> exit 1, error is logged, never swallowed.
 *   B6 Policy returns success:true -> exit 0.
 */
export async function runCli(
  avg: string[] = process.argv,
  io: CliIO = defaultIO
): Promise<number> {
  io.log("🚂 QuickLendX Migration Runner\n");

  const args = parseArgs(arg);
  const { command, error } = resolveCommand(args);

  if (error) {
    io.error(`❌ ${error}`);
    return 1;
  }

  try {
    const result: CliResult =
      command === "down"
        ? await migrateDownCommand(args)
        : await migrateCommand(args);

    if (result.success) {
      return 0;
    }

    io.error(`\n❌ ${result.message}`);
    return 1;
  } catch (err: any) {
    // Never echo the raw error object: it may carry connection strings or paths.
    // The message is the contract between the runner and the operator.
    const message = err instanceof Error ? err.message : String(err);
    io.error(`Unexpected error: ${message}`);
    return 1;
  }
}

// Only execute when invoked as a script. When imported by tests the exported
// runCli is exercised instead, so tests can assert on the exit code and output
// without killing the process.
if (require.main === module) {
  runCli().then((code) => {
    process.exitCode = code;
  });
}
