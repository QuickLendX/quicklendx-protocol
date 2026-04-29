# Financial Tables UX Guidelines

This document defines readability rules, formatting standards, and interaction patterns for financial data tables (Bids, Payments, Returns) within the QuickLendX platform.

## 1. Core Design Principles

- **Clarity over Density**: While high-density tables are useful for experts, clarity remains paramount. Use whitespace to group related information.
- **Financial Integrity**: Primary financial figures (Amounts, Yields) must be the most visually prominent elements.
- **Contextual Prioritization**: Column order and visibility should change based on the user's role (Business vs. Investor).
- **Security Transparency**: Never hide risks or fees. Distinguish between *actual* and *projected* values.

---

## 2. Global Formatting Standards

### 2.1 Currency & Numbers
- **Alignment**: Always **right-align** numerical currency columns to allow for easy decimal comparison.
- **Precision**: Show 2 decimal places by default. For tokens with high precision (e.g., XLM), show up to 4 significant decimals if the value is < 1.00.
- **Font**: Use a tabular (monospaced) font for numbers to ensure columns align perfectly.
- **Weight**: Bold the primary "Amount" column.

### 2.2 Addresses & Identifiers
- **Truncation**: Truncate long Stellar addresses (e.g., `GABC...XYZ1`).
- **Interaction**: Provide a "Click to Copy" affordance and a "Link to Explorer" icon.
- **Avatars**: Use Identicons or project-specific avatars next to addresses to help users visually distinguish counterparties.

### 2.3 Status Badges
Status indicators must use consistent color semantics:
- **Success (Green)**: `Accepted`, `Paid`, `Completed`, `Funded`.
- **Neutral/Warning (Yellow)**: `Placed`, `Pending`, `Under Review`.
- **Critical/Failure (Red)**: `Expired`, `Cancelled`, `Defaulted`, `Disputed`.
- **Informational (Blue)**: `Withdrawn`, `Refunded`.

---

## 3. Specific Table Guidelines

### 3.1 Bids Table
*Used by Businesses to review offers and Investors to track their open positions.*

| Priority | Column | Formatting | Sorting Logic |
| :--- | :--- | :--- | :--- |
| 1 | **Bid Amount** | Bold, Right-aligned | Asc/Desc |
| 2 | **Yield / Profit** | Percentage or Delta | Desc (Default for Business) |
| 3 | **Status** | Status Badge | Grouped by status |
| 4 | **Expiry** | Relative Time (e.g., "In 2 days") | Asc (Soonest first) |
| 5 | **Investor / Invoice** | Address with Avatar | N/A |
| 6 | **Placed Date** | Date & Time | Desc (Newest first) |

**Special Interaction: Rank Indicator**
Bids should include a "Rank" column (e.g., #1, #2) based on the contract's `compare_bids` logic (Profit -> Expected Return -> Amount -> Time).

### 3.2 Payment History
*Used for auditing cash flow and verifying settlements.*

| Priority | Column | Formatting | Sorting Logic |
| :--- | :--- | :--- | :--- |
| 1 | **Amount** | Bold Green (Inflow) / Gray (Outflow) | Asc/Desc |
| 2 | **Date** | Absolute Date (e.g., "Apr 28, 2024") | Desc (Default) |
| 3 | **Payer/Payee** | Address | N/A |
| 4 | **Tx ID** | Truncated Hash + Explorer Link | N/A |

### 3.3 Returns & Investments
*Focused on the investor's portfolio performance.*

| Priority | Column | Formatting | Sorting Logic |
| :--- | :--- | :--- | :--- |
| 1 | **Principal** | Amount | Asc/Desc |
| 2 | **Projected Return** | Amount + % Yield | Desc |
| 3 | **Due Date** | Countdown or Absolute Date | Asc (Default) |
| 4 | **Status** | Investment Status Badge | N/A |

---

## 4. Density & Navigation

### 4.1 Row Density
- **Standard**: 48px height. Best for primary dashboards.
- **Compact**: 32px height. Toggleable for "Audit Log" views or power users handling dozens of invoices.

### 4.2 "Show More" vs. Pagination
- **Show More (Infinite Scroll)**: Preferred for the "Activity Feed" and "Recent Bids" on a specific invoice page.
- **Pagination**: Mandatory for the "History" tab and "All My Bids" views where the user might need to jump to specific timeframes or pages.
- **Threshold**: Use "Show More" for the first 10-20 items; transition to pagination if the dataset exceeds 50 items.

### 4.3 Empty States
Never leave a table blank. Provide clear, actionable empty states:
- **No Bids**: "No bids yet. High-quality invoices usually receive bids within 24 hours."
- **No Payments**: "No payment history found for this invoice."
- **No Investments**: "You haven't made any investments yet. [Browse Invoices]"

---

## 5. Security & Risk Messaging

### 5.1 No Misleading Guarantees
- **Projected vs. Realized**: In the Returns table, "Projected Return" must be clearly labeled to distinguish it from "Realized Profit" (paid out).
- **Risk Tiers**: If the invoice has a Risk Tier (e.g., Tier A-E), display this badge prominently near the Bid Amount to contextualize the yield.
- **Tooltips**: Hovering over a status like `Defaulted` or `Disputed` must provide a direct link to the risk policy or dispute resolution steps.

### 5.2 Transaction Finality
- **Pending States**: When a transaction is submitted but not yet confirmed by the Stellar network, the row should appear in a "Ghost" or "Pulsing" state to indicate it is not yet immutable.
- **Confirmation Delay**: Display a subtle "Transaction may take 5-10 seconds to finalize" message during active operations.

### 5.3 Clear Risk Warnings
- High-yield bids (> X% above market average) should be flagged with a subtle "High Risk" icon to prevent "too good to be true" errors by businesses.
- **Security Check**: Always verify the "Expected Return" does not exceed the "Invoice Total" plus a reasonable interest cap; if it does, flag it as a potential logic error or outlier.
