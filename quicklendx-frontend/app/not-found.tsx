import Link from "next/link";

/**
 * Branded 404 page.
 *
 * Rendered by Next.js for unknown routes and for pages that call
 * `notFound()` — including the dynamic routes that reject malformed
 * identifiers before they hit the API.
 */
export default function NotFound() {
  return (
    <main className="min-h-screen bg-gray-50 flex items-center justify-center px-4 py-12">
      <div className="max-w-md w-full bg-white border border-[#E2E8F0] rounded-lg shadow-[0_1px_3px_rgba(0,0,0,0.1)] p-8 text-center">
        <div className="mb-4 flex justify-center text-[#94A3B8]">
          <svg
            width="56"
            height="56"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
          >
            <circle cx="12" cy="12" r="9" />
            <path d="M9.2 9.3a2.9 2.9 0 1 1 3.6 2.8c-.8.3-1 .9-1 1.6" />
            <path d="M12 17v.01" />
          </svg>
        </div>

        <p className="text-sm font-semibold text-[#2563EB] tracking-wider uppercase">
          Error 404
        </p>
        <h1 className="mt-2 text-2xl font-bold text-[#0F172A]">
          Page not found
        </h1>
        <p className="mt-3 text-[#64748B] leading-relaxed">
          We could not find the page you were looking for. It may have been
          moved, or the link may be incorrect.
        </p>

        <div className="mt-6 flex flex-col sm:flex-row gap-3 justify-center">
          <Link
            href="/"
            className="inline-flex items-center justify-center px-4 py-2 rounded-md bg-[#2563EB] text-white text-sm font-medium hover:brightness-110 focus:outline-none focus:ring-2 focus:ring-[#2563EB] focus:ring-offset-1"
          >
            Back to Home
          </Link>
        </div>
      </div>
    </main>
  );
}
