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

## Committed-secret scanning

CI and local security checks run a committed-secret scanner as part of `npm run security:scan`.

- Script: `backend/scripts/secret-scan.js`
- Allowlist: `backend/scripts/.secret-scan-allow.json`
- Coverage: `backend/src`, `backend/tests`, `backend/scripts`, and `backend/.env.example`

The scanner flags:

- High-entropy literals in quoted strings (for example random signing material or export tokens)
- Known secret formats:
  - QuickLendX API keys (`qlx_…`)
  - Stripe-style keys (`sk_live_…`, `sk_test_…`)
  - Slack bot tokens (`xoxb-…`)
  - AWS access keys (`AKIA…`)
  - Stellar secret seeds (`S…`, 56-character StrKey)

When a match is found, the gate exits non-zero and prints file, line, column, rule name, and a **redacted preview**. Full secret values are never written to CI logs.

To suppress a documented false positive, add an entry to `.secret-scan-allow.json` with the file, line, and reason. Do not use the allowlist for real secrets.

Run locally:

```bash
cd backend
node scripts/secret-scan.js
```

The scanner is also exercised by `backend/tests/secret-scan.test.ts` during `npm test`.

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
