# Visual Language Direction: QuickLendX Protocol

This document outlines the visual philosophy and UX principles for QuickLendX. The goal is to create an interface that feels like a secure, professional financial tool while maintaining the speed and transparency of the Stellar network.

## 1. Core UX Pillars

### 1.1 Trust through Transparency
Every action in QuickLendX involves financial risk. The UI must never obscure details or use "dark patterns."
- **Data Freshness**: Always display the last sync time for invoice data.
- **Explicit Estimations**: If a value (like interest or fees) is an estimate, it must be labeled `(est.)` with a tooltip explaining the variables.
- **Immutable Proof**: Every settlement or bid should provide a direct link to the Stellar Expert or Horizon explorer.

### 1.2 Security-First Communication
Security is not just in the code; it's in how the user perceives risk.
- **Risk Color Semantics**: Red is reserved for *loss* or *blockers*. Amber is for *precautions*. Never use brand colors for risk-sensitive alerts to avoid confusion.
- **No Misleading Guarantees**: Use terms like "Expected Yield" instead of "Guaranteed Return." Avoid using overly celebratory animations for risky investment actions.

---

## 2. Visual Language Patterns

### 2.1 Component Semantics
- **The "Safety Rail" Card**: High-value cards (Invoices, Bids) should use a subtle left-border color to indicate status (e.g., Emerald for `PAID`, Rose for `DEFAULTED`).
- **Confirmation Friction**: Destructive or high-value actions (Accepting a Bid, Canceling an Invoice) require a "Double-Click" or "Hold to Confirm" pattern to prevent accidental clicks.

### 2.2 Protocol State Representation
The UI must clearly distinguish between protocol-level states:

| Protocol State | Visual Treatment | Messaging |
| :--- | :--- | :--- |
| **Open/Active** | Clean, high-contrast typography | "Awaiting Bids" |
| **Locked/Settling** | Subtle "Glassmorphism" overlay | "Settlement in Progress - Assets Locked" |
| **Defaulted** | Desaturated background + High-contrast Rose text | "Protocol Default - Recovery Initiated" |

---

## 3. Security Assumptions & Validations

1. **Assumed**: Users understand that once a bid is accepted, the invoice is legally/on-chain committed. 
   - **Validation**: The "Accept Bid" modal must display a summary of the commitment: *"By accepting, you lock [Amount] XLM until [Date]."*
2. **Assumed**: Protocol fees are dynamic.
   - **Validation**: Fees must be broken down in a "Receipt-style" summary before any confirmation.
3. **Assumed**: Collateralization ratios can fluctuate.
   - **Validation**: Use a "Pulse" animation or color shift (Warning Amber) if collateral drops below 110% of the bid.

---

## 4. Visual "North Star"
> "QuickLendX should look like a high-end Bloomberg terminal meets modern DeFi simplicity. It should feel robust, precise, and devoid of unnecessary decorative fluff."
