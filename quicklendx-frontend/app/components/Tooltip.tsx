"use client";

/**
 * Tooltip — inline help per quicklendx-frontend/tooltips.md.
 *
 *  - Pointer (mouse): opens after `delay` (300ms) of hover; closes 100ms after
 *    the pointer leaves the trigger or the tooltip (section 2).
 *  - Keyboard: opens immediately on focus; closes on blur or Escape.
 *  - Touch: a tap on the trigger toggles it with no delay; a tap outside closes it.
 *  - Accessibility: trigger is a <button type="button"> with aria-describedby
 *    pointing at a role="tooltip" node while open.
 *  - Rendered into document.body with fixed positioning so ancestor
 *    overflow settings cannot clip it. Flips placement when the viewport
 *    has no room and clamps horizontally inside the viewport.
 *
 * No positioning library is used; the frontend has none installed.
 */

import React, {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";

export const TOOLTIP_OPEN_DELAY_MS = 300;
export const TOOLTIP_CLOSE_DELAY_MS = 100;

const OFFSET_PX = 8;
const VIEWPORT_MARGIN_PX = 16;

export interface TooltipProps {
  /** Tooltip body (definition, formula, last-updated, link). */
  content: React.ReactNode;
  /** Visible trigger label. Omit for an icon-only trigger. */
  children?: React.ReactNode;
  /** "icon" renders an info icon after the label; "underline" renders a dotted underline. */
  variant?: "icon" | "underline";
  /** Preferred placement. Flips to the other side when the viewport has no room. */
  placement?: "top" | "bottom";
  /** Hover open delay in ms. */
  delay?: number;
  /** Maximum tooltip width in px on large screens. */
  maxWidth?: number;
  /** Accessible name for the trigger. Required when `children` is omitted. */
  triggerLabel?: string;
  /** Extra classes for the trigger button. */
  className?: string;
  /** Optional id for the tooltip node. Generated when omitted. */
  id?: string;
}

interface Position {
  top: number;
  left: number;
  placement: "top" | "bottom";
}

export const Tooltip: React.FC<TooltipProps> = ({
  content,
  children,
  variant = "icon",
  placement = "top",
  delay = TOOLTIP_OPEN_DELAY_MS,
  maxWidth = 280,
  triggerLabel,
  className = "",
  id,
}) => {
  const generatedId = useId();
  const tooltipId = id ?? `${generatedId}-tip`;

  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState<Position | null>(null);

  const triggerRef = useRef<HTMLButtonElement>(null);
  const tooltipRef = useRef<HTMLDivElement>(null);
  const openTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const closeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Pointer type of the most recent press on the trigger. Used so a pointer
  // press does not open via focus; the click that follows handles it.
  const pressedPointerRef = useRef<string | null>(null);

  const cancelOpenTimer = useCallback(() => {
    if (openTimerRef.current !== null) {
      clearTimeout(openTimerRef.current);
      openTimerRef.current = null;
    }
  }, []);

  const cancelCloseTimer = useCallback(() => {
    if (closeTimerRef.current !== null) {
      clearTimeout(closeTimerRef.current);
      closeTimerRef.current = null;
    }
  }, []);

  const show = useCallback(() => {
    cancelOpenTimer();
    cancelCloseTimer();
    setOpen(true);
  }, [cancelOpenTimer, cancelCloseTimer]);

  const hide = useCallback(() => {
    cancelOpenTimer();
    cancelCloseTimer();
    setOpen(false);
  }, [cancelOpenTimer, cancelCloseTimer]);

  const scheduleClose = useCallback(() => {
    cancelCloseTimer();
    closeTimerRef.current = setTimeout(() => {
      closeTimerRef.current = null;
      setOpen(false);
    }, TOOLTIP_CLOSE_DELAY_MS);
  }, [cancelCloseTimer]);

  // Clear pending timers on unmount.
  useEffect(() => {
    return () => {
      cancelOpenTimer();
      cancelCloseTimer();
    };
  }, [cancelOpenTimer, cancelCloseTimer]);

  // While open: Escape dismisses, and a press outside trigger and tooltip dismisses.
  useEffect(() => {
    if (!open) return;

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") hide();
    };
    const onPointerDown = (event: Event) => {
      const target = event.target as Node | null;
      if (
        target &&
        (triggerRef.current?.contains(target) ||
          tooltipRef.current?.contains(target))
      ) {
        return;
      }
      hide();
    };

    document.addEventListener("keydown", onKeyDown);
    document.addEventListener("pointerdown", onPointerDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      document.removeEventListener("pointerdown", onPointerDown);
    };
  }, [open, hide]);

  // While open: position the tooltip against the trigger, flipping placement
  // and clamping horizontally so it stays inside the viewport.
  useLayoutEffect(() => {
    if (!open) {
      setPosition(null);
      return;
    }

    const update = () => {
      const trigger = triggerRef.current;
      const tip = tooltipRef.current;
      if (!trigger || !tip) return;

      const t = trigger.getBoundingClientRect();
      const r = tip.getBoundingClientRect();
      const vw = window.innerWidth;
      const vh = window.innerHeight;

      const topSpace = t.top - OFFSET_PX - r.height;
      const bottomSpace = t.bottom + OFFSET_PX + r.height;

      let place: "top" | "bottom" = placement;
      if (place === "top" && topSpace < 0 && bottomSpace <= vh) {
        place = "bottom";
      } else if (place === "bottom" && bottomSpace > vh && topSpace >= 0) {
        place = "top";
      }

      const top = place === "top" ? topSpace : t.bottom + OFFSET_PX;
      const centred = t.left + t.width / 2 - r.width / 2;
      const maxLeft = Math.max(VIEWPORT_MARGIN_PX, vw - r.width - VIEWPORT_MARGIN_PX);
      const left = Math.min(Math.max(centred, VIEWPORT_MARGIN_PX), maxLeft);

      setPosition({ top, left, placement: place });
    };

    update();
    window.addEventListener("resize", update);
    window.addEventListener("scroll", update, true);
    return () => {
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", update, true);
    };
  }, [open, placement]);

  // Pointer hover: mouse only. Touch uses tap, handled in onClick.
  const onPointerEnter = (event: React.PointerEvent<HTMLButtonElement>) => {
    if (event.pointerType !== "mouse") return;
    cancelCloseTimer();
    cancelOpenTimer();
    openTimerRef.current = setTimeout(() => {
      openTimerRef.current = null;
      setOpen(true);
    }, delay);
  };

  const onPointerLeave = (event: React.PointerEvent<HTMLButtonElement>) => {
    pressedPointerRef.current = null;
    if (event.pointerType !== "mouse") return;
    cancelOpenTimer();
    scheduleClose();
  };

  const onPointerDown = (event: React.PointerEvent<HTMLButtonElement>) => {
    pressedPointerRef.current = event.pointerType;
  };

  const onFocus = () => {
    if (pressedPointerRef.current === null) show();
  };

  const onBlur = () => {
    pressedPointerRef.current = null;
    hide();
  };

  const onClick = (event: React.MouseEvent<HTMLButtonElement>) => {
    pressedPointerRef.current = null;
    // detail === 0 means keyboard or assistive-tech activation: keep it open.
    if (event.detail === 0) {
      show();
      return;
    }
    // Pointer or touch tap: toggle. A tap on an open tooltip's trigger closes it.
    if (open) {
      hide();
    } else {
      show();
    }
  };

  // Hovering the tooltip keeps it open so the pointer can reach it.
  const onTooltipPointerEnter = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.pointerType !== "mouse") return;
    cancelCloseTimer();
  };

  const onTooltipPointerLeave = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.pointerType !== "mouse") return;
    scheduleClose();
  };

  const triggerClass =
    variant === "underline"
      ? "inline-flex min-h-[44px] items-center cursor-help border-b border-dotted border-gray-500"
      : "inline-flex items-center cursor-help text-gray-400 hover:text-gray-600 focus:outline-none focus-visible:ring-2 focus-visible:ring-gray-500 rounded-sm";

  const tooltipNode = open
    ? createPortal(
        <div
          ref={tooltipRef}
          id={tooltipId}
          role="tooltip"
          data-placement={position?.placement ?? placement}
          onPointerEnter={onTooltipPointerEnter}
          onPointerLeave={onTooltipPointerLeave}
          style={{
            position: "fixed",
            top: position?.top ?? 0,
            left: position?.left ?? 0,
            maxWidth: `min(${maxWidth}px, calc(100vw - 32px))`,
            visibility: position ? "visible" : "hidden",
            zIndex: 60,
          }}
          className="rounded-md bg-slate-900 px-3 py-2 text-xs leading-relaxed text-white shadow-lg"
        >
          {content}
        </div>,
        document.body
      )
    : null;

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        aria-label={triggerLabel}
        aria-describedby={open ? tooltipId : undefined}
        onPointerEnter={onPointerEnter}
        onPointerLeave={onPointerLeave}
        onPointerDown={onPointerDown}
        onFocus={onFocus}
        onBlur={onBlur}
        onClick={onClick}
        className={`${triggerClass} ${className}`.trim()}
      >
        {children}
        {variant === "icon" && (
          <svg
            className="h-3.5 w-3.5"
            fill="currentColor"
            viewBox="0 0 20 20"
            aria-hidden="true"
            focusable="false"
          >
            <path
              fillRule="evenodd"
              d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7-4a1 1 0 11-2 0 1 1 0 012 0zM9 9a1 1 0 000 2v3a1 1 0 001 1h1a1 1 0 100-2v-3a1 1 0 00-1-1H9z"
              clipRule="evenodd"
            />
          </svg>
        )}
      </button>
      {tooltipNode}
    </>
  );
};
