"use client";

/**
 * Dialog — shared accessible dialog primitive.
 *
 * WCAG 2.1 AA compliance:
 *  - SC 4.1.2: role="dialog" / "alertdialog", aria-modal, aria-labelledby, aria-describedby
 *  - SC 2.4.3: focus moves into dialog on open, returns to opener on every close path
 *  - SC 2.1.1: Tab/Shift+Tab cycle within dialog, Escape closes unless dismissible=false
 *  - Background is marked `inert` to prevent tabbable content behind the overlay
 *
 * WAI-ARIA APG modal dialog pattern:
 * https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/
 */

import React, {
  useCallback,
  useEffect,
  useId,
  useRef,
} from "react";
import { createPortal } from "react-dom";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface DialogProps {
  /** Whether the dialog is open. */
  isOpen: boolean;
  /** Called on every close path (Escape, backdrop click, explicit close). */
  onClose: () => void;
  /** Ref to the element that triggered the dialog — focus returns here on close. */
  triggerRef?: React.RefObject<HTMLElement | null>;
  /** Use "alertdialog" for destructive confirmations. Default: "dialog". */
  role?: "dialog" | "alertdialog";
  /** aria-labelledby value. If omitted, pass aria-label directly via ...rest. */
  labelId?: string;
  /** aria-describedby value. */
  descriptionId?: string;
  /** When false, Escape and backdrop clicks do not close the dialog. Default: true. */
  dismissible?: boolean;
  children: React.ReactNode;
  /** Extra className for the dialog panel wrapper. */
  panelClassName?: string;
  /** Extra className for the backdrop. */
  backdropClassName?: string;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const FOCUSABLE_SELECTORS = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  '[tabindex]:not([tabindex="-1"])',
  "details > summary",
].join(", ");

function getFocusableElements(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTORS)).filter(
    (el) => !el.closest("[inert]") && el.offsetParent !== null
  );
}

// ---------------------------------------------------------------------------
// Dialog component
// ---------------------------------------------------------------------------

export const Dialog: React.FC<DialogProps> = ({
  isOpen,
  onClose,
  triggerRef,
  role = "dialog",
  labelId,
  descriptionId,
  dismissible = true,
  children,
  panelClassName = "",
  backdropClassName = "",
}) => {
  const panelRef = useRef<HTMLDivElement>(null);
  const fallbackLabelId = useId();
  const resolvedLabelId = labelId ?? fallbackLabelId;

  // Focus the first focusable element when dialog opens
  useEffect(() => {
    if (!isOpen) return;
    // Defer until portal renders
    const frame = requestAnimationFrame(() => {
      const panel = panelRef.current;
      if (!panel) return;
      const focusable = getFocusableElements(panel);
      if (focusable.length > 0) {
        focusable[0].focus();
      } else {
        panel.focus();
      }
    });
    return () => cancelAnimationFrame(frame);
  }, [isOpen]);

  // Apply `inert` to siblings of the dialog root when open, remove when closed.
  // This prevents background content from being tabbable or read by AT.
  useEffect(() => {
    if (!isOpen) return;
    const portal = panelRef.current?.closest("[data-dialog-portal]") as HTMLElement | null;
    if (!portal) return;
    const parent = portal.parentElement;
    if (!parent) return;

    const siblings = Array.from(parent.children).filter(
      (el) => el !== portal
    ) as HTMLElement[];
    siblings.forEach((el) => el.setAttribute("inert", ""));
    return () => siblings.forEach((el) => el.removeAttribute("inert"));
  }, [isOpen]);

  // Stable close handler that always reads current onClose / triggerRef
  const onCloseRef = useRef(onClose);
  const triggerRefRef = useRef(triggerRef);
  useEffect(() => { onCloseRef.current = onClose; }, [onClose]);
  useEffect(() => { triggerRefRef.current = triggerRef; }, [triggerRef]);

  const handleClose = useCallback(() => {
    onCloseRef.current();
    // Return focus to the opener on the next frame, after the dialog unmounts
    requestAnimationFrame(() => {
      const target = triggerRefRef.current?.current;
      if (target && typeof target.focus === "function") {
        target.focus();
      }
    });
  }, []);

  // Keyboard handler: Tab trap + Escape
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      const panel = panelRef.current;
      if (!panel) return;

      if (e.key === "Escape") {
        if (dismissible) {
          e.stopPropagation();
          handleClose();
        }
        return;
      }

      if (e.key === "Tab") {
        const focusable = getFocusableElements(panel);
        if (focusable.length === 0) {
          e.preventDefault();
          return;
        }
        const first = focusable[0];
        const last = focusable[focusable.length - 1];

        if (e.shiftKey) {
          if (document.activeElement === first) {
            e.preventDefault();
            last.focus();
          }
        } else {
          if (document.activeElement === last) {
            e.preventDefault();
            first.focus();
          }
        }
      }
    },
    [dismissible, handleClose]
  );

  if (!isOpen) return null;

  return createPortal(
    <div data-dialog-portal>
      {/* Backdrop */}
      <div
        className={`fixed inset-0 bg-black/50 z-40 ${backdropClassName}`}
        aria-hidden="true"
        onClick={dismissible ? handleClose : undefined}
      />

      {/* Panel */}
      <div
        ref={panelRef}
        role={role}
        aria-modal="true"
        aria-labelledby={resolvedLabelId}
        aria-describedby={descriptionId}
        tabIndex={-1}
        onKeyDown={handleKeyDown}
        className={`fixed z-50 focus:outline-none ${panelClassName}`}
      >
        {typeof children === "function"
          ? (children as (args: { labelId: string; handleClose: () => void }) => React.ReactNode)({ labelId: resolvedLabelId, handleClose })
          : children}
      </div>
    </div>,
    document.body
  );
};

export default Dialog;
