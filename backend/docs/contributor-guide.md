# Backend contributor guide

The canonical backend is this package. It is an Express 5 application with
routes in `src/routes/`, request handling in `src/controllers/`, business logic
in `src/services/`, and SQLite migrations in `src/migrations/`.

## Development

From the repository root:

```sh
cd backend
npm ci
npm run dev
npm test
```

Run `npm run build` to type-check and build the service. The API application is
assembled in [`src/app.ts`](../src/app.ts), and the OpenAPI contract is in
[`openapi.yaml`](../openapi.yaml).

## Request path

Routes compose middleware and controller handlers. Controllers validate and
translate HTTP input, then call services for application behavior. Add new
endpoints by updating the appropriate route and controller, the OpenAPI
contract, and the related focused tests.

## Database changes

Add schema changes as a `MigrationDefinition` in `src/migrations/`, following
the existing numbered migration files and migration types. Validate changes
with the migration tests and the commands documented in the backend README.

## Existing service references

- Export requests and downloads are implemented by
  [`src/routes/v1/exports.ts`](../src/routes/v1/exports.ts) and
  [`src/controllers/v1/exports.ts`](../src/controllers/v1/exports.ts).
- Ingest-lag alerts and degraded-mode behavior are described in
  [`observability.md`](./observability.md).
- Secret scanning and local environment guidance are described in
  [`secrets.md`](./secrets.md).

The repository does not include the separate package's metrics service,
hot-reload configuration loader, or contract-test harness. See the root
documentation index for the current backend references.
