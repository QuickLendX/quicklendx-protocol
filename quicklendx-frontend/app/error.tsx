"use client";

import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { ErrorFallback } from "./components/ErrorBoundary";

/**
 * Segment-level error boundary.
 *
 * Next.js renders this component instead of the segment that threw. `reset()`
 * re-renders only the failed segment, so client state (form input, wallet
 * session) survives — the page is never reloaded.
 */
export default function AppError({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  const router = useRouter();

  useEffect(() => {
    // Log the error for diagnostics. In production this is where a remote
    // reporting service would be called.
    console.error("Unhandled error in route segment:", error);
  }, [error]);

  return (
    <ErrorFallback
      error={error}
      resetErrorBoundary={reset}
      onReauthenticate={() => router.push("/")}
      showReloadButton={false}
    />
  );
}
