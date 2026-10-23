"use client";

import { useEffect } from "react";
import { ErrorFallback } from "./components/ErrorBoundary";

/**
 * Root error boundary.
 *
 * This replaces the root layout when an error escapes every segment boundary,
 * so it has to render its own `<html>` and `<body>`.
 */
export default function GlobalError({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  useEffect(() => {
    console.error("Unhandled root error:", error);
  }, [error]);

  return (
    <html lang="en">
      <body>
        <ErrorFallback
          error={error}
          resetErrorBoundary={reset}
          showReloadButton={false}
        />
      </body>
    </html>
  );
}
