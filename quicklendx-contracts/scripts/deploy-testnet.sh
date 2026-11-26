#!/usr/bin/env bash
set -euo pipefail

CONTRACTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$CONTRACTS_DIR"

usage() {
  cat <<'EOF'
Usage: scripts/deploy-testnet.sh [--dry-run] [--help]

Build and optimize the contract, deploy it to a non-mainnet network, initialize
the protocol, verify initialization, and save NETWORK and CONTRACT_ID to
.env.deploy.

Environment:
  NETWORK          testnet (default), futurenet, or local
  IDENTITY         Stellar CLI identity used to deploy and initialize (default: admin)
  ADMIN_ADDRESS    Public address for the initial protocol admin (required)
  TREASURY_ADDRESS Public address for the protocol treasury (required)
  CONTRACT_ALIAS   Stellar CLI alias (default: quicklendx-$NETWORK)
  INITIAL_CURRENCY Optional token contract address to whitelist at initialization

Protocol initialization uses fee_bps=200, min_invoice_amount=1000000,
max_due_date_days=365, grace_period_seconds=604800, and
backfill_max_batch_size=100. Corridors are empty initially.
EOF
}

DRY_RUN=0
while (($#)); do
  case "$1" in
    --dry-run) DRY_RUN=1 ;;
    --help|-h) usage; exit 0 ;;
    *) printf 'Unknown option: %s\n' "$1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

NETWORK="${NETWORK:-testnet}"
IDENTITY="${IDENTITY:-admin}"
CONTRACT_ALIAS="${CONTRACT_ALIAS:-quicklendx-${NETWORK}}"
ADMIN_ADDRESS="${ADMIN_ADDRESS:-}"
TREASURY_ADDRESS="${TREASURY_ADDRESS:-}"
INITIAL_CURRENCY="${INITIAL_CURRENCY:-}"
ENV_FILE="$CONTRACTS_DIR/.env.deploy"

if [[ "$NETWORK" == "mainnet" || "$NETWORK" == "public" ]]; then
  printf 'Refusing to deploy to mainnet. Use testnet, futurenet, or local.\n' >&2
  exit 1
fi

case "$NETWORK" in
  testnet|futurenet|local) ;;
  *) printf 'Unsupported NETWORK "%s"; use testnet, futurenet, or local.\n' "$NETWORK" >&2; exit 1 ;;
esac

if [[ ! "$CONTRACT_ALIAS" =~ ^[A-Za-z0-9._-]+$ ]]; then
  printf 'CONTRACT_ALIAS may contain only letters, numbers, ".", "_" and "-".\n' >&2
  exit 1
fi
if [[ -n "$INITIAL_CURRENCY" && ! "$INITIAL_CURRENCY" =~ ^C[A-Z2-7]{55}$ ]]; then
  printf 'INITIAL_CURRENCY must be a valid C... Stellar token contract address.\n' >&2
  exit 1
fi

if [[ "$DRY_RUN" -eq 0 ]]; then
  for required in ADMIN_ADDRESS TREASURY_ADDRESS; do
    if [[ -z "${!required}" ]]; then
      printf '%s must be set to a Stellar public address.\n' "$required" >&2
      exit 1
    fi
    if [[ ! "${!required}" =~ ^G[A-Z2-7]{55}$ ]]; then
      printf '%s must be a valid G... Stellar public address.\n' "$required" >&2
      exit 1
    fi
  done
  if ! command -v stellar >/dev/null 2>&1; then
    printf 'The Stellar CLI is required. Install stellar-cli@23.0.0; see CONTRIBUTING.md.\n' >&2
    exit 1
  fi
else
  ADMIN_ADDRESS="${ADMIN_ADDRESS:-<ADMIN_ADDRESS>}"
  TREASURY_ADDRESS="${TREASURY_ADDRESS:-<TREASURY_ADDRESS>}"
fi

CURRENCIES_JSON='[]'
if [[ -n "$INITIAL_CURRENCY" ]]; then
  CURRENCIES_JSON="$(printf '["%s"]' "$INITIAL_CURRENCY")"
fi
PARAMS="$(printf '{"admin":"%s","treasury":"%s","fee_bps":200,"min_invoice_amount":"1000000","max_due_date_days":365,"grace_period_seconds":604800,"initial_currencies":%s,"corridors":[],"backfill_max_batch_size":100}' "$ADMIN_ADDRESS" "$TREASURY_ADDRESS" "$CURRENCIES_JSON")"
WASM_PATH="target/wasm32v1-none/release/quicklendx_contracts.wasm"

print_command() {
  printf '+'
  printf ' %q' "$@"
  printf '\n'
}

if [[ "$DRY_RUN" -eq 1 ]]; then
  printf 'Dry run: no build, network request, or file write will be performed.\n'
  print_command stellar contract build --optimize
  print_command stellar contract deploy --wasm "$WASM_PATH" --source-account "$IDENTITY" --network "$NETWORK" --alias "$CONTRACT_ALIAS"
  print_command stellar contract invoke --id '<CONTRACT_ID>' --source-account "$IDENTITY" --network "$NETWORK" -- initialize --params "$PARAMS"
  print_command stellar contract invoke --id '<CONTRACT_ID>' --source-account "$IDENTITY" --network "$NETWORK" -- is_initialized
  printf '+ write NETWORK=%s, CONTRACT_ID=<deployed contract ID>, and INITIAL_CURRENCY=%s to %s\n' \
    "$NETWORK" "${INITIAL_CURRENCY:-<none>}" "$ENV_FILE"
  exit 0
fi

printf '==> Building and optimizing the contract for %s...\n' "$NETWORK"
stellar contract build --optimize
if [[ ! -f "$WASM_PATH" ]]; then
  WASM_PATH="target/wasm32-unknown-unknown/release/quicklendx_contracts.wasm"
  if [[ ! -f "$WASM_PATH" ]]; then
    printf 'Built WASM not found at the expected Stellar target paths. Check the CLI build output.\n' >&2
    exit 1
  fi
fi

printf '==> Deploying with Stellar identity "%s"...\n' "$IDENTITY"
if ! deploy_output="$(stellar contract deploy \
  --wasm "$WASM_PATH" \
  --source-account "$IDENTITY" \
  --network "$NETWORK" \
  --alias "$CONTRACT_ALIAS" 2>&1)"; then
  printf '%s\n' "$deploy_output" >&2
  exit 1
fi
printf '%s\n' "$deploy_output"

contract_ids="$(printf '%s\n' "$deploy_output" | grep -Eo 'C[A-Z2-7]{55}' | sort -u || true)"
if [[ -z "$contract_ids" || "$contract_ids" == *$'\n'* ]]; then
  printf 'Could not extract exactly one contract ID from the deploy output.\n' >&2
  exit 1
fi
CONTRACT_ID="$contract_ids"

umask 077
env_tmp="$(mktemp "$CONTRACTS_DIR/.env.deploy.XXXXXX")"
trap 'rm -f -- "$env_tmp"' EXIT
printf 'NETWORK=%s\nCONTRACT_ID=%s\n' "$NETWORK" "$CONTRACT_ID" >"$env_tmp"
if [[ -n "$INITIAL_CURRENCY" ]]; then
  printf 'CURRENCY_ADDRESS=%s\n' "$INITIAL_CURRENCY" >>"$env_tmp"
fi
mv -f -- "$env_tmp" "$ENV_FILE"
trap - EXIT
printf '==> Saved NETWORK and CONTRACT_ID to %s\n' "$ENV_FILE"

printf '==> Initializing protocol...\n'
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source-account "$IDENTITY" \
  --network "$NETWORK" \
  -- initialize --params "$PARAMS"

printf '==> Verifying protocol initialization...\n'
if ! initialized="$(stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source-account "$IDENTITY" \
  --network "$NETWORK" \
  -- is_initialized 2>&1)"; then
  printf '%s\n' "$initialized" >&2
  exit 1
fi
printf '%s\n' "$initialized"
if ! printf '%s\n' "$initialized" | grep -Eiq 'true'; then
  printf 'Initialization verification failed: is_initialized did not return true.\n' >&2
  exit 1
fi

printf 'Deployment complete: %s\n' "$CONTRACT_ID"
