# KYC Document Upload UI/UX Specification

## Widget Anatomy
- **Drag‑and‑Drop Area**: Large rectangle with subtle glass‑morphism background, supporting both mouse and keyboard (focusable). Drop zone displays an upload icon and instructional text.
- **File Picker Button**: Accessible button labeled "Select files" that opens the native file dialog.
- **Thumbnail Preview**: After selection, a thumbnail (or file‑type icon) appears with file name, size, and a clear‑styled remove (**✕**) button.
- **Validation Messages**: Inline error messages appear underneath the preview with ARIA‑describedby linking.

## Document Slots (General KYC)
| Slot | Required | Accepted Formats | Max Size |
|------|----------|------------------|----------|
| Government ID | ✔︎ | PDF, PNG, JPG | 5 MB |
| Proof of Address | ✔︎ | PDF, PNG, JPG | 5 MB |
| Business Registration (if applicable) | ✖︎ | PDF, PNG, JPG | 5 MB |
| Ownership Declaration (if applicable) | ✖︎ | PDF, PNG, JPG | 5 MB |

## Status Chip Set
| Status | Icon (Heroicons) | Text (Calm Tone) | ARIA Announcement |
|--------|------------------|------------------|-------------------|
| Not Started | `document-duplicate` | "Not started" | "Document not started" |
| Uploaded | `document-upload` | "Uploaded – pending review" | "Document uploaded, pending review" |
| In Review | `eye` | "In review" | "Document is being reviewed" |
| Verified | `check-circle` | "Verified" | "Document verified" |
| Rejected | `x-circle` | "Rejected – see reason" | "Document rejected, reason announced" |

*All chips use a minimum AA contrast ratio and include a subtle motion‑fade when state changes.*

## Micro‑Interactions
- **File Validation**: Immediate feedback if file type or size exceeds limits. Error text is linked via `aria-describedby` to the input.
- **Status Transitions**: Fade‑in/out animation (150 ms) when a chip changes state.
- **Resubmission Flow**: When a document is **Rejected**, the chip shows a red outline and an **“Update Document”** button appears next to the thumbnail. Clicking it opens the file picker again for that slot.

## Accessibility (Reference: `docs/ux/business-dashboard.md`)
- Keyboard focus order: Drop zone → File picker → Individual document rows.
- All interactive elements have discernible `aria-label`s.
- Errors are programmatically linked to their inputs with `aria-describedby`.
- Status chips are announced via an ARIA live region (`polite`).

## Responsive Design
- **375 px**: Single‑column layout, full‑width drop zone, stacked document rows.
- **1280 px**: Two‑column layout, drop zone on the left, document list on the right, allowing side‑by‑side preview.

## Copy Guidelines
- Use calm, specific language; avoid phrases implying guaranteed approval (e.g., “awaiting verification” instead of “will be approved”).
- Follow tone examples in `docs/ux/defaults-spec.md`.

---
*Prepared for branch `uiux/kyc-document-upload-spec`.*
