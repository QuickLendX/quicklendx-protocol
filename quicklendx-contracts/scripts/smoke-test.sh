#!/usr/bin/env bash
set -euo pipefail

CONTRACTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="$CONTRACTS_DIR/.env.deploy"

usage() {
  cat <<'EOF'
Usage: scripts/smoke-test.sh [--dry-run] [--help]

Exercise invoice creation, verification, bidding, funding, and escrow release.
The deployment account file is read from .env.deploy.

Environment:
  NETWORK                 testnet, futurenet, or local (defaults to .env.deploy)
  CONTRACT_ID             deployed contract ID (defaults to .env.deploy)
  ADMIN_IDENTITY          Stellar CLI identity for the protocol admin (default: admin)
  BUSINESS_IDENTITY       Stellar CLI identity that owns BUSINESS_ADDRESS
  BUSINESS_ADDRESS        public address for the verified business
  INVESTOR_IDENTITY       Stellar CLI identity that owns INVESTOR_ADDRESS
  INVESTOR_ADDRESS        public address for the verified investor
  CURRENCY_ADDRESS        initialized, supported token contract address
  INVOICE_AMOUNT          amount in token base units (default: 1000000)
  BID_AMOUNT              bid in token base units (default: INVOICE_AMOUNT)
  EXPECTED_RETURN         expected return in token base units (default: BID_AMOUNT)
  DUE_DATE_OFFSET_SECONDS future due-date offset (default: 2592000)

The business and investor must already be verified. The investor identity must
be able to fund the bid, and the token must be initialized and supported by the
contract. --dry-run prints the complete command plan without network requests.
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

DEPLOY_NETWORK=""
DEPLOY_CONTRACT_ID=""
DEPLOY_CURRENCY_ADDRESS=""
if [[ -f "$ENV_FILE" ]]; then
  while IFS='=' read -r key value; do
    case "$key" in
      NETWORK) DEPLOY_NETWORK="$value" ;;
      CONTRACT_ID) DEPLOY_CONTRACT_ID="$value" ;;
      CURRENCY_ADDRESS) DEPLOY_CURRENCY_ADDRESS="$value" ;;
    esac
  done <"$ENV_FILE"
fi

NETWORK="${NETWORK:-${DEPLOY_NETWORK:-testnet}}"
CONTRACT_ID="${CONTRACT_ID:-$DEPLOY_CONTRACT_ID}"
ADMIN_IDENTITY="${ADMIN_IDENTITY:-admin}"
BUSINESS_IDENTITY="${BUSINESS_IDENTITY:-}"
BUSINESS_ADDRESS="${BUSINESS_ADDRESS:-}"
INVESTOR_IDENTITY="${INVESTOR_IDENTITY:-}"
INVESTOR_ADDRESS="${INVESTOR_ADDRESS:-}"
CURRENCY_ADDRESS="${CURRENCY_ADDRESS:-$DEPLOY_CURRENCY_ADDRESS}"
INVOICE_AMOUNT="${INVOICE_AMOUNT:-1000000}"
BID_AMOUNT="${BID_AMOUNT:-$INVOICE_AMOUNT}"
EXPECTED_RETURN="${EXPECTED_RETURN:-$BID_AMOUNT}"
DUE_DATE_OFFSET_SECONDS="${DUE_DATE_OFFSET_SECONDS:-2592000}"

if [[ "$NETWORK" == "mainnet" || "$NETWORK" == "public" ]]; then
  printf 'Refusing to run the smoke test on mainnet. Use testnet, futurenet, or local.\n' >&2
  exit 1
fi
case "$NETWORK" in
  testnet|futurenet|local) ;;
  *) printf 'Unsupported NETWORK "%s"; use testnet, futurenet, or local.\n' "$NETWORK" >&2; exit 1 ;;
esac

if [[ "$DRY_RUN" -eq 0 ]]; then
  for required in CONTRACT_ID BUSINESS_IDENTITY BUSINESS_ADDRESS INVESTOR_IDENTITY INVESTOR_ADDRESS CURRENCY_ADDRESS; do
    if [[ -z "${!required}" ]]; then
      printf '%s is required. See scripts/smoke-test.sh --help.\n' "$required" >&2
      exit 1
    fi
  done
  if [[ ! "$CONTRACT_ID" =~ ^C[A-Z2-7]{55}$ ]]; then
    printf 'CONTRACT_ID must be a valid C... Stellar contract address.\n' >&2
    exit 1
  fi
  for address_name in BUSINESS_ADDRESS INVESTOR_ADDRESS CURRENCY_ADDRESS; do
    if [[ ! "${!address_name}" =~ ^[GC][A-Z2-7]{55}$ ]]; then
      printf '%s must be a valid G... account or C... contract address.\n' "$address_name" >&2
      exit 1
    fi
  done
  if ! command -v stellar >/dev/null 2>&1; then
    printf 'The Stellar CLI is required. Install stellar-cli@23.0.0; see CONTRIBUTING.md.\n' >&2
    exit 1
  fi
else
  CONTRACT_ID="${CONTRACT_ID:-<CONTRACT_ID>}"
  BUSINESS_IDENTITY="${BUSINESS_IDENTITY:-business}"
  BUSINESS_ADDRESS="${BUSINESS_ADDRESS:-<BUSINESS_ADDRESS>}"
  INVESTOR_IDENTITY="${INVESTOR_IDENTITY:-investor}"
  INVESTOR_ADDRESS="${INVESTOR_ADDRESS:-<INVESTOR_ADDRESS>}"
  CURRENCY_ADDRESS="${CURRENCY_ADDRESS:-<CURRENCY_ADDRESS>}"
fi

for amount_name in INVOICE_AMOUNT BID_AMOUNT EXPECTED_RETURN DUE_DATE_OFFSET_SECONDS; do
  if [[ ! "${!amount_name}" =~ ^[0-9]+$ ]] || [[ "${!amount_name}" -le 0 ]]; then
    printf '%s must be a positive integer.\n' "$amount_name" >&2
    exit 1
  fi
done

if [[ "$DRY_RUN" -eq 1 ]]; then
  printf 'Dry run: no network request will be performed.\n'
  printf '+ stellar contract invoke --id %q --source-account %q --network %q -- store_invoice --business %q --amount %q --currency %q --due-date <UNIX_TIMESTAMP> --description %q --category Services --tags %q --origination-fee-bps null\n' \
    "$CONTRACT_ID" "$BUSINESS_IDENTITY" "$NETWORK" "$BUSINESS_ADDRESS" "$INVOICE_AMOUNT" "$CURRENCY_ADDRESS" "QuickLendX smoke test" '[]'
  printf '+ stellar contract invoke --id %q --source-account %q --network %q -- verify_invoice --invoice-id <INVOICE_ID>\n' "$CONTRACT_ID" "$ADMIN_IDENTITY" "$NETWORK"
  printf '+ stellar contract invoke --id %q --source-account %q --network %q --send no -- get_invoice --invoice-id <INVOICE_ID> # assert verified status\n' "$CONTRACT_ID" "$ADMIN_IDENTITY" "$NETWORK"
  printf '+ stellar contract invoke --id %q --source-account %q --network %q -- place_bid --investor %q --invoice-id <INVOICE_ID> --bid-amount %q --expected-return %q --salt <32-byte-hex>\n' \
    "$CONTRACT_ID" "$INVESTOR_IDENTITY" "$NETWORK" "$INVESTOR_ADDRESS" "$BID_AMOUNT" "$EXPECTED_RETURN"
  printf '+ stellar contract invoke --id %q --source-account %q --network %q -- accept_bid --invoice-id <INVOICE_ID> --bid-id <BID_ID>\n' "$CONTRACT_ID" "$INVESTOR_IDENTITY" "$NETWORK"
  printf '+ stellar contract invoke --id %q --source-account %q --network %q --send no -- get_escrow_status --invoice-id <INVOICE_ID> # assert Held\n' "$CONTRACT_ID" "$ADMIN_IDENTITY" "$NETWORK"
  printf '+ stellar contract invoke --id %q --source-account %q --network %q -- release_escrow_funds --invoice-id <INVOICE_ID>\n' "$CONTRACT_ID" "$BUSINESS_IDENTITY" "$NETWORK"
  printf '+ stellar contract invoke --id %q --source-account %q --network %q --send no -- get_escrow_status --invoice-id <INVOICE_ID> # assert Released\n' "$CONTRACT_ID" "$ADMIN_IDENTITY" "$NETWORK"
  exit 0
fi

extract_bytes32() {
  local output="$1"
  local label="$2"
  local value
  value="$(printf '%s\n' "$output" | grep -Eo '(0x)?[[:xdigit:]]{64}' | tail -n 1 || true)"
  if [[ -z "$value" ]]; then
    printf 'Could not parse %s from Stellar CLI output; expected a 32-byte hexadecimal result.\n' "$label" >&2
    return 1
  fi
  printf '%s' "$value"
}

invoke() {
  local description="$1"
  shift
  local output
  printf '==> %s\n' "$description" >&2
  if ! output="$("$@" 2>&1)"; then
    printf '%s\n' "$output" >&2
    return 1
  fi
  if [[ -z "$output" ]]; then
    printf 'Stellar CLI returned no output for: %s\n' "$description" >&2
    return 1
  fi
  printf '%s\n' "$output" >&2
  printf '%s' "$output"
}

base_args=(--id "$CONTRACT_ID" --network "$NETWORK")
DUE_DATE="$(( $(date +%s) + DUE_DATE_OFFSET_SECONDS ))"
SMOKE_SALT="0x0000000000000000000000000000000000000000000000000000000000000001"

invoice_output="$(invoke 'Creating invoice' stellar contract invoke "${base_args[@]}" --source-account "$BUSINESS_IDENTITY" -- \
  store_invoice --business "$BUSINESS_ADDRESS" --amount "$INVOICE_AMOUNT" \
  --currency "$CURRENCY_ADDRESS" --due-date "$DUE_DATE" \
  --description 'QuickLendX smoke test' --category Services --tags '[]' \
  --origination-fee-bps null)"
INVOICE_ID="$(extract_bytes32 "$invoice_output" 'invoice ID')"

invoke 'Verifying invoice' stellar contract invoke "${base_args[@]}" --source-account "$ADMIN_IDENTITY" -- \
  verify_invoice --invoice-id "$INVOICE_ID" >/dev/null
invoice_state="$(invoke 'Checking verified invoice' stellar contract invoke "${base_args[@]}" \
  --source-account "$ADMIN_IDENTITY" --send no -- get_invoice --invoice-id "$INVOICE_ID")"
if ! printf '%s\n' "$invoice_state" | grep -Eiq 'Verified'; then
  printf 'Invoice verification assertion failed: get_invoice did not report Verified.\n' >&2
  exit 1
fi

bid_output="$(invoke 'Placing bid' stellar contract invoke "${base_args[@]}" --source-account "$INVESTOR_IDENTITY" -- \
  place_bid --investor "$INVESTOR_ADDRESS" --invoice-id "$INVOICE_ID" \
  --bid-amount "$BID_AMOUNT" --expected-return "$EXPECTED_RETURN" --salt "$SMOKE_SALT")"
BID_ID="$(extract_bytes32 "$bid_output" 'bid ID')"

invoke 'Accepting bid and funding escrow' stellar contract invoke "${base_args[@]}" \
  --source-account "$INVESTOR_IDENTITY" -- --send yes accept_bid \
  --invoice-id "$INVOICE_ID" --bid-id "$BID_ID" >/dev/null
escrow_state="$(invoke 'Checking funded escrow' stellar contract invoke "${base_args[@]}" \
  --source-account "$ADMIN_IDENTITY" --send no -- get_escrow_status --invoice-id "$INVOICE_ID")"
if ! printf '%s\n' "$escrow_state" | grep -Eiq 'Held'; then
  printf 'Funding assertion failed: get_escrow_status did not report Held.\n' >&2
  exit 1
fi

invoke 'Releasing escrow to the business' stellar contract invoke "${base_args[@]}" \
  --source-account "$BUSINESS_IDENTITY" -- --send yes release_escrow_funds \
  --invoice-id "$INVOICE_ID" >/dev/null
escrow_state="$(invoke 'Checking released escrow' stellar contract invoke "${base_args[@]}" \
  --source-account "$ADMIN_IDENTITY" --send no -- get_escrow_status --invoice-id "$INVOICE_ID")"
if ! printf '%s\n' "$escrow_state" | grep -Eiq 'Released'; then
  printf 'Release assertion failed: get_escrow_status did not report Released.\n' >&2
  exit 1
fi

printf 'Smoke test passed for invoice %s and bid %s.\n' "$INVOICE_ID" "$BID_ID"
