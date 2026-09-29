/**
 * Manual mock for the `pg` module.
 *
 * The `pg` package is not installed as a production dependency in this
 * workspace (the project uses better-sqlite3 for local storage). However
 * src/services/database.ts imports it and is transitively required by
 * src/controllers/v1/bids.ts → SnapshotService.
 *
 * This mock stubs out the Pool class so the contract-test suite can import
 * app.ts without a running PostgreSQL instance.
 *
 * All Pool methods return resolved Promises so they behave safely even if
 * test code accidentally calls them.
 */

const mockRelease = jest.fn();
const mockQuery = jest.fn().mockResolvedValue({ rows: [], rowCount: 0 });
const mockConnect = jest.fn().mockResolvedValue({
  query: mockQuery,
  release: mockRelease,
});

export class Pool {
  connect = mockConnect;
  query = mockQuery;
  end = jest.fn().mockResolvedValue(undefined);
}

export default { Pool };
