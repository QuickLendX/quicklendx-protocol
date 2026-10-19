/**
 * Fixture that cannot be loaded at all: it throws while being `require`d.
 *
 * Used to prove that `loadMigrationsFromFS` wraps loader failures in a message
 * that names the offending file, so a bad migration is diagnosable.
 */

throw new Error("fixture intentionally fails to load");

export default undefined as unknown as Record<string, unknown>;