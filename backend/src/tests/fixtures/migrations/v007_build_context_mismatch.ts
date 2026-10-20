import type { MigrationDefinition } from "../../../lib/migrations/types";

/**
 * Fixture whose exported version deliberately disagrees with its filename.
 *
 * The filename parses as version 7; the definition below claims version 8. Used
 * to prove `loadMigrationsFromFS` rejects the mismatch instead of applying a
 * migration under a version the bookkeeping table never recorded.
 */
const definition: MigrationDefinition = {
  version: 8,
  name: "build_context_mismatch",
  authoredAt: "2026-01-01T00:00:00Z",
  author: "quicklendx-tests",
  up: async (): Promise<void> => undefined,
};

export default definition;