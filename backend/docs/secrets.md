# Secrets Management

## Local development

1. Copy the template and fill in values:
   ```bash
   cp backend/.env.example backend/.env
   ```
2. `.env` is gitignored — never commit it.
3. Install the pre-commit hook (one-time, after cloning):
   ```bash
   bash scripts/install-hooks.sh
   ```
   The hook scans staged files for patterns that look like real secrets and blocks the commit if any are found.

## Environment variables

| Variable | Required in prod | Default | Notes |
|---|---|---|---|
| `PORT` | no | `3001` | |
| `NODE_ENV` | no | `development` | `development` \| `test` \| `production` |
| `STELLAR_RPC_URL` | no | testnet URL | Override for mainnet |
| `RATE_LIMIT_POINTS` | no | `100` | Requests per IP per minute |
| `ADMIN_API_KEY` | yes | — | Min 32 chars; set via CI secret |
| `WEBHOOK_SECRET` | yes | — | Min 16 chars; set via CI secret |

Config is validated at startup via `src/config.ts` (zod). The app throws immediately if required vars are missing, listing field names only — values are never logged.

## Staging / production

Set secrets through your CI/CD provider (GitHub Actions secrets, AWS SSM, etc.). Never hardcode values in workflow files or source code.

```yaml
# GitHub Actions example
- run: npm start
  env:
    ADMIN_API_KEY: ${{ secrets.ADMIN_API_KEY }}
    WEBHOOK_SECRET: ${{ secrets.WEBHOOK_SECRET }}
    NODE_ENV: production
```

## Safe overrides

To override a single variable without editing `.env`, prefix the command:

```bash
STELLAR_RPC_URL=https://soroban-mainnet.stellar.org npm run dev
```
