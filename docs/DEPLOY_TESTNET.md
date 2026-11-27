# Test-network deployment and smoke test

These scripts build and optimize the Soroban contract, deploy and initialize it,
then exercise an invoice-to-escrow flow using the Stellar CLI. They deliberately
refuse `mainnet` and `public`; they are not production deployment tools.

## Prerequisites

- Bash, Rust/Cargo, and Stellar CLI `23.0.0` (see
  [contract contributor prerequisites](../quicklendx-contracts/CONTRIBUTING.md)).
- A configured Stellar CLI network named `testnet`, `futurenet`, or `local`.
  Configure any custom local RPC URL and passphrase in the Stellar CLI network
  settings before selecting `local`.
- Funded CLI identities for the deployer/admin, business, and investor.
- The admin and treasury Stellar public addresses.
- For the smoke test, an initialized and supported token contract plus verified
  business and investor accounts. The investor must have enough token balance
  and authorization to fund the bid.

All commands below run from `quicklendx-contracts/`.

## Deploy and initialize

Set the network and initial protocol addresses, then run:

```bash
NETWORK=testnet \
IDENTITY=admin \
ADMIN_ADDRESS=G... \
TREASURY_ADDRESS=G... \
INITIAL_CURRENCY=C... \
bash scripts/deploy-testnet.sh
```

`IDENTITY` is the Stellar CLI identity that deploys the contract and signs
initialization; its public address must match `ADMIN_ADDRESS`. `NETWORK` is one
of `testnet` (the default), `futurenet`, or `local`. `CONTRACT_ALIAS` optionally
overrides the CLI alias (default `quicklendx-$NETWORK`). The script initializes
with a 2% fee, minimum invoice amount `1000000`, 365 maximum due-date days, a
604800-second grace period, a 100-item backfill limit, and an empty corridor
list. `INITIAL_CURRENCY` optionally whitelists a token contract address during
initialization and is saved as `CURRENCY_ADDRESS` for the smoke script. It
verifies `is_initialized` and writes `NETWORK` and `CONTRACT_ID` (plus the
optional token address) to the ignored `quicklendx-contracts/.env.deploy` file.

Preview all build, deploy, initialize, verification, and file-write steps
without running commands:

```bash
NETWORK=testnet bash scripts/deploy-testnet.sh --dry-run
```

## Run the invoice smoke flow

After deployment, supply the already-configured account and token details:

```bash
BUSINESS_IDENTITY=business \
BUSINESS_ADDRESS=G... \
INVESTOR_IDENTITY=investor \
INVESTOR_ADDRESS=G... \
CURRENCY_ADDRESS=C... \
bash scripts/smoke-test.sh
```

The script reads the network and contract ID from `.env.deploy` unless
`NETWORK` or `CONTRACT_ID` is set explicitly. `ADMIN_IDENTITY` defaults to
`admin`. `INVOICE_AMOUNT` defaults to `1000000`; `BID_AMOUNT` defaults to the
invoice amount; `EXPECTED_RETURN` defaults to the bid amount; and
`DUE_DATE_OFFSET_SECONDS` defaults to 30 days. Amounts are token base units.
The script checks invoice verification and the escrow's `Held` and `Released`
states, and fails if a CLI call fails or a state assertion does not match.

Preview the complete smoke flow without sending transactions:

```bash
bash scripts/smoke-test.sh --dry-run
```

## Troubleshooting

- **Unsupported network / mainnet refusal:** choose `testnet`, `futurenet`, or
  `local`. Mainnet use is intentionally blocked.
- **Identity or funding errors:** confirm each identity exists in the Stellar
  CLI config and has enough native XLM for fees. Fund testnet accounts using
  the testnet friendbot or your configured funding service.
- **Initialization authorization fails:** the identity signing deploy and
  initialization must correspond to `ADMIN_ADDRESS`.
- **Invoice or bid authorization fails:** confirm business/investor verification
  and pass the matching identity/address pair.
- **Bid funding fails:** check the configured currency contract, investor
  balance, and required token authorization for the contract.
- **Contract ID cannot be parsed:** retain the CLI deployment output and verify
  the CLI reports the contract ID as a 32-byte hexadecimal `C...` address. The
  deployment is not repeated automatically.
- **Build output missing:** run `stellar contract build --optimize` manually
  and check the WASM output path before retrying.
