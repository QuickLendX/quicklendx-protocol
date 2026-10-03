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
 *
 * Note: uses plain functions instead of jest.fn() because jest is not
 * installed as a dependency in this package.
 */

const mockReleaseFn = () => undefined;
const mockQueryFn = () => Promise.resolve({ rows: [], rowCount: 0 });
const mockConnectFn = () => Promise.resolve({
  query: mockQueryFn,
  release: mockReleaseFn,
});

export class Pool {
  connect = mockConnectFn;
  query = mockQueryFn;
  end = () => Promise.resolve(undefined);
}

export default { Pool };
