# Review Notes & Design Decisions: UX Tokens

This document captures the rationale behind the design token selections and visual direction for QuickLendX.

## 1. Decision Log

| Decision | Rationale |
| :--- | :--- |
| **Base-8 Spacing System** | Standardizes layout consistency and ensures vertical rhythm across complex data tables. |
| **Trust Blue (Primary 600)** | Selected to convey stability and institutional reliability, essential for a lending protocol. |
| **Strict Semantic Separation** | Reserve specific hues (Rose, Amber, Emerald) purely for risk and status. This prevents "color fatigue" where users ignore warnings because they look like brand elements. |
| **Monospaced Data** | High-precision numbers and addresses require constant character width to remain readable in dense dashboard views. |
| **Trust & Safety Pattern Library** | Irreversible on-chain actions now require risk classification, explicit consequence copy, safe defaults, and stronger confirmation for critical fund-routing or authority changes. |

## 2. Security Considerations
- **No Ambiguity**: Values are explicitly marked as estimates where applicable.
- **Visual Friction**: Destructive actions are visually distinct from constructive ones (e.g., "Cancel" uses a flat outline while "Confirm" uses a solid fill).
- **Audit Trails**: Visual links to block explorers are treated as "First Class" components, not hidden in sub-menus.
- **No Misleading Guarantees**: Confirmations and risk banners must not promise safety, recovery, yield, insurance payout, or successful settlement.
- **Freshness Before Signing**: High-risk and critical actions must disclose data freshness and pause signing when the current on-chain state cannot be verified.

## 3. Next Steps (Implementation)
- Implement these tokens as CSS variables in `globals.css`.
- Map Tailwind configuration to these tokens.
- Apply semantic states to the `InvoiceCard` and `BidForm` components.
- Apply [trust-safety-pattern-library.md](./trust-safety-pattern-library.md) when designing confirmations, wallet handoffs, risk banners, and error recovery states.
