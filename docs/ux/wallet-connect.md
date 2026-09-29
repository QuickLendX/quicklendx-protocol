# Wallet Connect Modal & Connection-State UX Spec

**Issue:** #1018  
**Scope:** UI/UX specification only — no wallet SDK integration.  
**References:** `docs/ux/design-tokens.md`, `docs/ux/visual-direction.md`, `docs/ux/explorer-link.md`, `docs/ux/modals.md`

---

## 1. State Diagram

```
                    ┌─────────────────┐
                    │  NOT_CONNECTED  │◄──────────────────────┐
                    └────────┬────────┘                       │
                             │ User clicks "Connect Wallet"   │
                             ▼                                │
                    ┌─────────────────┐                       │
                    │   CONNECTING    │                       │
                    └────────┬────────┘                       │
                    ┌────────┴────────┐                       │
                    │                 │                       │
              (success)          (failure)                    │
                    │                 │                       │
                    ▼                 ▼                       │
           ┌──────────────┐  ┌──────────────┐                │
           │  CONNECTED   │  │    ERROR     │                │
           └──────┬───────┘  └──────┬───────┘                │
                  │                 │ User dismisses          │
           ┌──────┴───────┐         └───────────────────────►┘
           │              │
     (wrong network)  (disconnect)
           │              │
           ▼              ▼
  ┌──────────────┐  ┌──────────────────┐
  │ WRONG_NETWORK│  │  DISCONNECTING   │
  └──────┬───────┘  └────────┬─────────┘
         │                   │ (complete)
         │ User switches      └──────────────────────────────►┐
         │ network                                            │
         └──────────────────────────────────────────────────►┘
```

### State Definitions

| State | Description |
|---|---|
| `NOT_CONNECTED` | No wallet detected or user has not connected. Default entry state. |
| `CONNECTING` | Wallet extension prompt is open; awaiting user approval. |
| `CONNECTED` | Wallet approved; public key available; correct network. |
| `WRONG_NETWORK` | Wallet connected but network does not match expected (Mainnet/Testnet). |
| `ERROR` | Connection attempt failed (extension not found, user rejected, timeout). |
| `DISCONNECTING` | Disconnect in progress (clearing session state). |

---

## 2. Modal Anatomy

The wallet connect modal follows the base modal spec in `docs/ux/modals.md`.

```
┌──────────────────────────────────────────────────┐  ← elevation-high shadow
│ Connect Wallet                              [×]  │  ← text-h2, close button
├──────────────────────────────────────────────────┤  ← 1px color-neutral-200
│                                                  │
│  [Freighter icon]  Freighter                     │  ← wallet option row
│  Connect your Stellar wallet to continue.        │  ← text-body, neutral-500
│                                                  │
│  ─────────────────────────────────────────────  │
│                                                  │
│  🔒 Your keys never leave your device.           │  ← trust message
│     QuickLendX cannot access your funds.         │
│                                                  │
├──────────────────────────────────────────────────┤  ← 1px color-neutral-200
│                        [Cancel]  [Connect]       │  ← footer, right-aligned
└──────────────────────────────────────────────────┘
```

### Token Mapping

| Element | Token | Value |
|---|---|---|
| Background | — | `#FFFFFF` |
| Border radius | — | `8px` |
| Shadow | `elevation-high` | `0 10px 15px -3px rgba(0,0,0,0.1)` |
| Padding | `spacing-lg` | `24px` |
| Title | `text-h2` | `24px / 600` |
| Separator | `color-neutral-200` | `#E2E8F0` |
| Focus ring | `color-primary-600` | `#2563EB` |
| Backdrop | — | `rgba(0,0,0,0.5)` |

### Size Variants

| Viewport | Width | Border Radius |
|---|---|---|
| Desktop (≥ 768px) | `480px` max-width | `8px` |
| Mobile (< 768px) | Full-width, bottom sheet | `8px 8px 0 0` |

---

## 3. Per-State UI Treatment

### 3.1 NOT_CONNECTED

**Trigger point:** Nav bar "Connect Wallet" button or any gated action.

- Button label: **"Connect Wallet"**
- Button variant: Primary (`color-primary-600` background)
- No address shown in nav

### 3.2 CONNECTING

- Modal remains open; primary button replaced with a spinner + **"Connecting…"** label
- Primary button disabled (`opacity-40`, `cursor-not-allowed`)
- Body copy: *"Approve the connection request in your Freighter extension."*
- Cancel button remains active so the user can abort

### 3.3 CONNECTED

Modal closes. Nav bar updates to show:

```
[●] GBBX…r4Kz  [copy icon]  [↗ explorer icon]
```

- Green dot (`color-success-500 #10B981`) indicates live connection
- Address rendered in **Data Mono** (`JetBrains Mono`) — see Section 4
- Copy icon triggers clipboard copy with "Copied!" feedback (see Section 4)
- Explorer icon opens Stellar Expert in new tab (`rel="noopener noreferrer"`)
- Clicking the address row opens the **account menu** (disconnect option)

### 3.4 WRONG_NETWORK

Inline banner replaces modal body (or appears as a persistent top banner):

```
⚠️  Wrong network detected.
    Switch to [Testnet / Mainnet] in Freighter to continue.
    [Switch Network]  [Disconnect]
```

- Banner background: `color-warning-500` (`#F59E0B`) at 10% opacity, amber border
- "Switch Network" deep-links to Freighter's network settings if the API supports it; otherwise opens Freighter extension
- No gated actions are accessible while in this state

### 3.5 ERROR

Inline error replaces modal body:

```
✕  Could not connect.
   [Specific reason — see table below]
   [Try Again]  [Cancel]
```

| Error Cause | User-Facing Message |
|---|---|
| Extension not installed | "Freighter is not installed. [Get Freighter ↗]" |
| User rejected | "Connection was declined. You can try again at any time." |
| Timeout | "The request timed out. Please try again." |
| Unknown | "Something went wrong. Please try again." |

- Error text: `color-danger-500` (`#EF4444`)
- "Get Freighter" link opens `https://freighter.app` in new tab

### 3.6 DISCONNECTING

- Account menu "Disconnect" button shows spinner + **"Disconnecting…"** label
- Disabled during transition
- On completion → transitions to `NOT_CONNECTED`; nav reverts to "Connect Wallet" button

---

## 4. Address Display Rules

### 4.1 Truncation

Stellar public keys are 56 characters (G…). Always truncate in the middle:

```
Full:      GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ
Truncated: GBBXK…r4Kz
```

**Rule:** Show first **6** characters + `…` + last **4** characters.

```
function truncateAddress(address: string): string {
  if (address.length <= 13) return address;
  return `${address.slice(0, 6)}…${address.slice(-4)}`;
}
```

### 4.2 Typography

- Font: **JetBrains Mono** (`font-mono` in Tailwind)
- Size: `text-sm` (14px) in nav; `text-base` (16px) in modals/receipts
- Color: `color-neutral-900` (`#0F172A`)

### 4.3 Copy-to-Clipboard

1. User clicks copy icon adjacent to truncated address.
2. Full un-truncated address is written to `navigator.clipboard`.
3. Icon swaps to a checkmark for **2 seconds**, then reverts.
4. Screen-reader announcement: `aria-live="polite"` region emits *"Address copied."*

### 4.4 Explorer Link

- Destination: `https://stellar.expert/explorer/[network]/account/[address]`
  - `network` = `public` (Mainnet) or `testnet`
- Opens in new tab: `target="_blank" rel="noopener noreferrer"`
- Icon: external-link SVG (16×16) immediately after truncated address
- `aria-label`: `"View account [full address] on Stellar Expert (opens in new tab)"`

---

## 5. Trust & Safety Messaging

Per the "Security-First Communication" pillar in `docs/ux/visual-direction.md`:

| ✅ Use | ❌ Avoid |
|---|---|
| "Your keys never leave your device." | "We keep your wallet safe." |
| "QuickLendX cannot access your funds." | "Your funds are secured by QuickLendX." |
| "Approve in your Freighter extension." | "We're connecting your wallet." |
| "Disconnect" | "Log out" (implies server-side session) |

The trust message must appear in the modal body on every connection attempt, not just the first time.

---

## 6. Accessibility Requirements

| Requirement | Implementation |
|---|---|
| Focus trap | When modal is open, Tab cycles only within modal |
| Escape to close | `keydown` listener on `Escape` dismisses modal (except CONNECTING state — show confirmation) |
| Initial focus | First focusable element in modal (wallet option or "Connect" button) |
| ARIA role | `role="dialog"`, `aria-modal="true"`, `aria-labelledby` pointing to title |
| Close button | `aria-label="Close wallet connect modal"` |
| Spinner | `role="status"`, `aria-label="Connecting…"` |
| Live region | `aria-live="polite"` for copy feedback and state transitions |
| Color contrast | All text meets WCAG AA (4.5:1 for normal, 3:1 for large) |

---

## 7. Visual QA Checklist

- [ ] Modal renders correctly at **375px** (mobile bottom sheet)
- [ ] Modal renders correctly at **1280px** (desktop centered)
- [ ] Address truncation correct for 56-char Stellar keys
- [ ] Copy feedback visible and auto-resets after 2s
- [ ] Explorer link opens new tab with correct URL
- [ ] Wrong-network banner does not block modal close
- [ ] Error messages match the table in Section 3.5
- [ ] Focus trap active when modal is open
- [ ] Escape key closes modal (with confirmation prompt during CONNECTING)
- [ ] All interactive elements have visible focus rings (`color-primary-600`)
- [ ] Trust message visible on every open of the modal

---

## 8. Future Component Placement

The React implementation of this spec belongs at:

```
quicklendx-frontend/app/components/WalletConnect.tsx
```

See the placeholder component at that path for the prop interface and state machine scaffold. The full implementation will require the Freighter browser extension SDK (`@stellar/freighter-api`), which is out of scope for this spec.
