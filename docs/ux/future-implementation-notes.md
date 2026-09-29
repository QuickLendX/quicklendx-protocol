# Future Implementation Notes

**Target Location**: `app/onboarding/kyc/page.tsx`

**Component**: `KycDocumentUpload.tsx`

### Props (suggested)
```tsx
interface KycDocumentUploadProps {
  /** Optional callback when all required documents are uploaded and verified */
  onAllVerified?: () => void;
}
```

### State Shape (example)
```tsx
type DocumentSlot = {
  id: string; // e.g., "government-id"
  name: string; // Display name
  required: boolean;
  file?: File; // Currently selected file
  status: 'not-started' | 'uploaded' | 'in-review' | 'verified' | 'rejected';
  rejectionReason?: string;
};
```

### Status‑Chip Token Usage
Refer to the design token `statusChip` defined in `docs/ux/design-tokens.md`. Example usage:
```tsx
<StatusChip status={doc.status} icon="heroicons:document-upload" />
```

### Interaction Flow
1. **Upload** – User selects a file → widget validates format/size → status becomes `uploaded`.
2. **Submit for Review** – Automatically triggers backend KYC verification (not in scope).
3. **In Review** – Chip shows “In review” with subtle spinner.
4. **Verified** – Chip changes to green with check‑circle icon.
5. **Rejected** – Chip turns red, shows reason, and displays **“Update Document”** button to replace the file.

### Accessibility
- Wrap each document row in a `<section aria-labelledby="doc‑{id}-label">`.
- Use an ARIA live region (`role="status" aria-live="polite"`) inside the status chip component.
- Ensure the **Update Document** button is focusable and announces its purpose via `aria-label`.

### Styling References
- Follow glass‑morphism for the drag‑and‑drop container (see `docs/ux/visual-direction.md`).
- Use the status‑chip color palette from `docs/ux/design-tokens.md` (primary, success, warning, error).
- Include subtle motion‑fade transitions (150 ms) on status changes.

### Testing (post‑implementation)
- Unit tests for component state transitions using React Testing Library.
- Visual regression tests at 375 px and 1280 px.
- Accessibility audit using axe‑core (expect no violations on keyboard navigation, ARIA labels, and color contrast).

---
*Prepared for branch `uiux/kyc-document-upload-spec`.*
