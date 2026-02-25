# Repository Guidelines

## Project Structure & Module Organization
This repository is a monorepo with two main packages:
- `quicklendx-contracts/`: Soroban smart contracts in Rust (`src/` modules such as `invoice.rs`, `bid.rs`, `settlement.rs`), plus contract tests and WASM size checks.
- `quicklendx-frontend/`: Next.js App Router frontend (`app/`, `app/components/`, `app/lib/`, `public/`).

Project-level docs live in `docs/` and root `*.md` files. CI workflows are in `.github/workflows/`.

## Build, Test, and Development Commands
Run commands from the relevant package directory unless noted.

- Contracts:
  - `cargo build` (or `make build`): build Rust contract.
  - `cargo test`: run unit/integration tests.
  - `cargo test --features fuzz-tests fuzz_`: run proptest fuzz tests.
  - `./scripts/check-wasm-size.sh`: enforce WASM size budget (256 KB).
- Frontend:
  - `npm ci`: install exact dependencies.
  - `npm run dev`: start local dev server.
  - `npm run build`: production build (includes type checks).
  - `npm run lint`: run Next.js ESLint checks.
  - `npx tsc --noEmit` and `npx prettier --check .`: match frontend CI checks.
- Root helper:
  - `./run_fuzz_tests.sh [quick|standard|extended|thorough]`.

## Coding Style & Naming Conventions
- Rust: use `cargo fmt --all`; keep modules and functions `snake_case`, types/enums `PascalCase`.
- TypeScript/React: follow Next.js conventions; component files in `PascalCase` (e.g., `ErrorBoundary.tsx`), helpers in `camelCase` (e.g., `api-client.ts`); prefer 2-space indentation in TS/JSON.
- Keep functions focused, validate inputs, and avoid hardcoded secrets or environment-specific values.

## Testing Guidelines
- Add/extend `cargo test` coverage for contract logic changes.
- For risk-sensitive contract paths, include fuzz tests under the `fuzz-tests` feature.
- Frontend currently relies on lint, type-check, and build validation in CI; add targeted tests when introducing complex UI/state behavior.

## Commit & Pull Request Guidelines
- Follow observed Conventional Commit prefixes: `feat:`, `fix:`, `test:`, `docs:`, `chore:`.
- Keep commit scope specific (example: `test: add dispute lifecycle edge cases`).
- PRs should use `.github/pull_request_template.md`, link related issues (`Closes #...`), summarize testing performed, and include screenshots for UI changes.
- Ensure contract build/WASM checks and frontend lint/type/build checks pass before requesting review.
