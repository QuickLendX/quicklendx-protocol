# Emoji Usage Guide

## Audience: Contributors

This document defines where and how emojis may be used in QuickLendX UI copy. QuickLendX maintains a calm, objective, and precise tone for financial tooling. Excessive or improper use of emojis can distract users or misrepresent financial risk.

## General Principles

- **Use sparingly**: Emojis should not replace well-written copy.
- **Do not use emojis for status indicators**: Use explicit UI elements (like a green checkmark icon component or a red warning badge component) rather than unicode emojis (✅, ❌) which render inconsistently across operating systems and devices.
- **No emotion-inducing emojis**: Avoid using emojis that imply celebration (🎉, 🚀) or panic (🚨, 😱) in financial contexts.

## Where Emojis are Allowed

1. **Empty States & Light-hearted Contexts**
   In empty states where no active risk or financial transaction is occurring, a subtle emoji may be used to soften the UI.
   *Example:*
   > "You haven't uploaded any invoices yet. 📄"

2. **Marketing & Landing Pages**
   On non-transactional marketing pages, emojis can be used to break up large blocks of text or draw attention to specific features.
   *Example:*
   > "Secure your liquidity today. 💧"

## Where Emojis are Strictly Prohibited

1. **Transaction & Settlement Screens**
   Never use emojis when presenting financial data, transaction statuses, yields, or settlement details.
   *Example of what to avoid:*
   > "Your settlement was successful! 💸🚀"
   *Use instead:*
   > "Settlement completed."

2. **Error Messages & Alerts**
   Error states must remain objective. Do not try to soften an error with a sad face or use sirens to indicate failure.
   *Example of what to avoid:*
   > "Invoice could not be found 😢"
   *Use instead:*
   > "Invoice could not be found. Check the invoice ID or refresh."

3. **Dispute & Default Contexts**
   Disputes and defaults are serious legal and financial matters. Emojis trivialize these states.
   *Example of what to avoid:*
   > "This invoice has been marked defaulted. 🛑"
   *Use instead:*
   > "This invoice has been marked defaulted."

## Discoverability & Generated Docs

This guide applies primarily to UI copy. If you are writing generated documentation (such as JSDoc for frontend components), you can reference this guide for component text props:

```ts
/**
 * Renders an empty state message.
 * @param {string} message - The message to display. May contain a subtle emoji per docs/ux/emojis.md.
 */
function EmptyState({ message }: { message: string }) {
  // ...
}
```
