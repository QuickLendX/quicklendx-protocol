# QuickLendX Protocol — Formal Verification Model Notes

> [!IMPORTANT]
> **Target Audience:** Smart Contract Formal Verification Engineers, Security Auditors, and Protocol Verification Contributors.
> This document formalizes the state space, safety invariants, transition predicates, and formal mathematical properties of the QuickLendX Soroban smart contracts for symbolic execution, model checking (TLA+ / Certora / K-Framework), and property-based fuzz testing.

---

## 1. Formal State Space Definition

Let the protocol state $\mathcal{S}$ at ledger height $L$ and ledger timestamp $T$ be represented as a tuple:

$$\mathcal{S} = (\mathcal{I}, \mathcal{B}, \mathcal{E}, \mathcal{V}, \mathcal{A}, \mathcal{F}, \mathcal{P})$$

Where:
- $\mathcal{I}$: Finite map of Invoice IDs to Invoice records $\mathcal{I} : \text{BytesN}(32) \to \text{Invoice}$
- $\mathcal{B}$: Finite map of Bid IDs to Bid records $\mathcal{B} : \text{BytesN}(32) \to \text{Bid}$
- $\mathcal{E}$: Finite map of Escrow IDs to Escrow records $\mathcal{E} : \text{BytesN}(32) \to \text{Escrow}$
- $\mathcal{V}$: Finite map of Investment IDs to Investment records $\mathcal{V} : \text{BytesN}(32) \to \text{Investment}$
- $\mathcal{A}$: Admin authorization state $\mathcal{A} \in \text{Address}$
- $\mathcal{F}$: Protocol pause and maintenance flags $\mathcal{F} \in \{\text{Paused}, \text{Maintenance}, \text{Normal}\}$
- $\mathcal{P}$: Token balances held by the contract address $\mathcal{P} : \text{Address} \to \mathbb{N}$

---

## 2. Fundamental Safety Invariants

### Invariant 1: Invoice State Machine Transition Safety

Let $S_{invoice} \in \{\text{Pending}, \text{Verified}, \text{Funded}, \text{Paid}, \text{Defaulted}, \text{Cancelled}, \text{Refunded}\}$ be the status of an invoice $i \in \mathcal{I}$.

$$\forall i \in \mathcal{I}, \quad s_{next} \in \text{Transitions}(s_{current})$$

$$\begin{aligned}
\text{Transitions}(\text{Pending}) &= \{\text{Verified}, \text{Cancelled}\} \\
\text{Transitions}(\text{Verified}) &= \{\text{Funded}, \text{Cancelled}\} \\
\text{Transitions}(\text{Funded}) &= \{\text{Paid}, \text{Defaulted}, \text{Refunded}\} \\
\text{Transitions}(\text{Paid}) &= \emptyset \quad (\text{Terminal}) \\
\text{Transitions}(\text{Defaulted}) &= \emptyset \quad (\text{Terminal}) \\
\text{Transitions}(\text{Cancelled}) &= \emptyset \quad (\text{Terminal}) \\
\text{Transitions}(\text{Refunded}) &= \emptyset \quad (\text{Terminal})
\end{aligned}$$

**Formal Predicate**:
$$\text{IsTerminal}(s) \equiv s \in \{\text{Paid}, \text{Defaulted}, \text{Cancelled}, \text{Refunded}\}$$
$$\forall i \in \mathcal{I}, \quad \text{IsTerminal}(i.status) \implies \square (i.status = i.status_{final})$$

---

### Invariant 2: Escrow Mapping Uniqueness (Injectivity & Single-Slot Escrow)

Let $\text{EscrowForInvoice}(i\_id) = \{ e \in \mathcal{E} \mid e.invoice\_id = i\_id \}$.

$$\forall i\_id \in \text{Dom}(\mathcal{I}), \quad |\text{EscrowForInvoice}(i\_id)| \le 1$$

$$\forall e \in \mathcal{E}, \quad \mathcal{I}(e.invoice\_id) \neq \bot \land \mathcal{I}(e.invoice\_id).status \in \{\text{Funded}, \text{Paid}, \text{Defaulted}, \text{Refunded}\}$$

---

### Invariant 3: Settlement Accounting Identity & Value Conservation

For every invoice $i \in \mathcal{I}$ that enters status $\text{Paid}$:

$$i.total\_paid = \text{InvestorReturn}(i) + \text{PlatformFee}(i)$$

Where:
$$\text{PlatformFee}(i) = \lfloor \text{GrossProfit}(i) \times \text{FeeRate} \rfloor$$
$$\text{InvestorReturn}(i) = i.total\_paid - \text{PlatformFee}(i)$$

**Non-Negativity Constraint**:
$$\forall i \in \mathcal{I}_{Paid}, \quad \text{InvestorReturn}(i) \ge \text{Principal}(i) \land \text{PlatformFee}(i) \ge 0$$

---

### Invariant 4: Local Solvency & No Over-Funding

$$\forall i \in \mathcal{I}_{\text{Funded}}, \quad 0 < i.funded\_amount \le i.amount$$

$$\forall v \in \mathcal{V}_{\text{Active}}, \quad v.amount > 0$$

---

### Invariant 5: Global Solvency Inequality

The sum of all active investment principals across the protocol cannot exceed the sum of face values of all recorded invoices:

$$\sum_{v \in \mathcal{V}_{\text{Active}}} v.amount \le \sum_{i \in \mathcal{I}} i.amount$$

---

### Invariant 6: Storage Index Bijection & Coherence

Let $\text{Index}(s)$ be the set of invoice IDs stored in the status index for status $s$.

$$\forall s_1, s_2 \in \text{InvoiceStatus}, \quad s_1 \neq s_2 \implies \text{Index}(s_1) \cap \text{Index}(s_2) = \emptyset$$

$$\bigcup_{s \in \text{InvoiceStatus}} \text{Index}(s) = \text{Dom}(\mathcal{I})$$

$$\forall s \in \text{InvoiceStatus}, \forall i\_id \in \text{Index}(s), \quad \mathcal{I}(i\_id).status = s$$

---

### Invariant 7: Emergency Timelock & Reserve Protection

For any pending emergency withdrawal request $W$:

$$W.amount \le \text{TokenBalance}(W.token) - \text{HeldEscrowReserve}(W.token)$$

$$T_{\text{execution}} \ge W.unlock\_at \land T_{\text{execution}} \le W.expires\_at \land W.cancelled = \text{false}$$

---

## 3. Formal State Transition Pre- and Post-Conditions

### Transition: `accept_bid(env, admin, bid_id)`

**Pre-Conditions**:
1. Protocol circuit breaker is active-free: $\mathcal{F} = \text{Normal}$.
2. Bid exists: $b = \mathcal{B}(bid\_id) \neq \bot$.
3. Invoice exists: $i = \mathcal{I}(b.invoice\_id) \neq \bot$.
4. Invoice is ready: $i.status = \text{Verified}$.
5. No existing escrow: $|\text{EscrowForInvoice}(i.id)| = 0$.
6. Bid is active & unexpired: $b.status = \text{Active} \land T < b.expires\_at$.
7. Investor balance: $\text{TokenBalance}_{b.investor}(b.token) \ge b.amount$.

**Post-Conditions**:
1. $i'.status = \text{Funded} \land i'.funded\_amount = b.amount$.
2. New escrow created: $e_{new} \in \mathcal{E}' \land e_{new}.invoice\_id = i.id \land e_{new}.amount = b.amount$.
3. Token transfer executed: $\text{ContractBalance}(b.token)' = \text{ContractBalance}(b.token) + b.amount$.
4. Storage index updated: $i.id \notin \text{Index}(\text{Verified})' \land i.id \in \text{Index}(\text{Funded})'$.

---

## 4. Soroban Verification Entrypoint Code Example

The following Rust code snippet demonstrates how formal properties are asserted directly using Soroban SDK primitives in property-based verification tests:

```rust
use soroban_sdk::{Address, Env};
use quicklendx_contracts::invariants::{run_invariant_checks, InvariantReport};
use quicklendx_contracts::errors::QuickLendXError;

/// Formal verification harness: Asserts all 8 core protocol invariants hold on state S.
pub fn verify_protocol_invariants_formal(env: &Env) -> Result<(), QuickLendXError> {
    let report: InvariantReport = run_invariant_checks(env);
    
    // Formal Property 1: All checks must pass
    if !report.all_passed {
        return Err(QuickLendXError::InvariantViolation);
    }
    
    // Formal Property 2: Verification timestamp must be current
    assert_eq!(report.checked_at, env.ledger().timestamp());
    
    Ok(())
}

/// Verification Lemma: Ensures settlement accounting identity holds algebraically.
pub fn verify_settlement_identity_lemma(
    investment_amount: i128,
    total_paid: i128,
    investor_return: i128,
    platform_fee: i128,
) -> bool {
    if investment_amount <= 0 || total_paid < investment_amount {
        return false;
    }
    investor_return + platform_fee == total_paid && platform_fee >= 0 && investor_return >= investment_amount
}
```

---

## 5. Formal Model Verification Roadmap

| Model Component | Verification Tool | Property Type | Target Module |
| :--- | :--- | :--- | :--- |
| **State Machine Transitions** | TLA+ / TLC Model Checker | Safety & Liveness ($\square \diamond$) | `invoice.rs`, `settlement.rs` |
| **Settlement Accounting** | K-Framework / Certora | Value Conservation ($\sum \Delta = 0$) | `fees.rs`, `profits.rs` |
| **Escrow Uniqueness** | Rust Proptest / Kani | Invariant Invariance | `escrow.rs`, `payments.rs` |
| **Emergency Timelocks** | Temporal Logic Assertions | Bounded Timelock ($\le T$) | `emergency.rs`, `pause.rs` |
