"use client";

import { useCallback, useEffect, useState } from "react";
import toast from "react-hot-toast";
import { useWallet } from "@/app/lib/wallet/WalletProvider";
import {
  getNotificationPreferences,
  NotificationType,
  type NotificationPreferences,
  updateNotificationPreferences,
} from "./_lib/data";

const DEFAULT_PREFERENCES: NotificationPreferences = {
  email_enabled: true,
  notifications: {
    [NotificationType.InvoiceFunded]: true,
    [NotificationType.PaymentReceived]: true,
    [NotificationType.DisputeOpened]: true,
    [NotificationType.DisputeResolved]: true,
  },
};

const NOTIFICATION_OPTIONS: {
  type: NotificationType;
  label: string;
  description: string;
}[] = [
  {
    type: NotificationType.InvoiceFunded,
    label: "Invoice funded",
    description: "When an investor funds one of your invoices.",
  },
  {
    type: NotificationType.PaymentReceived,
    label: "Payment received",
    description: "When a payment is recorded against an invoice.",
  },
  {
    type: NotificationType.DisputeOpened,
    label: "Dispute opened",
    description: "When a dispute is opened for an invoice.",
  },
  {
    type: NotificationType.DisputeResolved,
    label: "Dispute resolved",
    description: "When a dispute has been resolved.",
  },
];

function isNotFound(error: unknown): boolean {
  if (!error || typeof error !== "object") return false;
  const candidate = error as {
    status?: number;
    code?: string;
    context?: { additionalData?: { status?: number } };
  };
  return (
    candidate.status === 404 ||
    candidate.code === "NOT_FOUND" ||
    candidate.context?.additionalData?.status === 404
  );
}

function errorMessage(error: unknown): string {
  return error instanceof Error && error.message
    ? error.message
    : "Unable to save notification preferences. Please try again.";
}

export default function NotificationPreferencesPage() {
  const { address } = useWallet();
  const [preferences, setPreferences] =
    useState<NotificationPreferences>(DEFAULT_PREFERENCES);
  const [savedPreferences, setSavedPreferences] =
    useState<NotificationPreferences>(DEFAULT_PREFERENCES);
  const [email, setEmail] = useState("");
  const [emailError, setEmailError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);

  const loadPreferences = useCallback(async () => {
    if (!address) {
      setLoading(false);
      setLoadError(null);
      return;
    }

    setLoading(true);
    setLoadError(null);
    try {
      const loaded = await getNotificationPreferences(address);
      setPreferences(loaded);
      setSavedPreferences(loaded);
      setEmail(loaded.email_address ?? "");
    } catch (error) {
      if (isNotFound(error)) {
        setPreferences(DEFAULT_PREFERENCES);
        setSavedPreferences(DEFAULT_PREFERENCES);
        setEmail("");
      } else {
        const message = errorMessage(error);
        setLoadError(message);
        toast.error(message);
      }
    } finally {
      setLoading(false);
    }
  }, [address]);

  useEffect(() => {
    let active = true;
    if (!address) {
      setLoading(false);
      setLoadError(null);
      return () => {
        active = false;
      };
    }

    setLoading(true);
    setLoadError(null);
    getNotificationPreferences(address)
      .then((loaded) => {
        if (!active) return;
        setPreferences(loaded);
        setEmail(loaded.email_address ?? "");
      })
      .catch((error: unknown) => {
        if (!active) return;
        if (isNotFound(error)) {
          setPreferences(DEFAULT_PREFERENCES);
          setSavedPreferences(DEFAULT_PREFERENCES);
          setEmail("");
          return;
        }
        const message = errorMessage(error);
        setLoadError(message);
        toast.error(message);
      })
      .finally(() => {
        if (active) setLoading(false);
      });

    return () => {
      active = false;
    };
  }, [address]);

  const handleSave = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!address || saving) return;

    const trimmedEmail = email.trim();
    if (trimmedEmail && !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(trimmedEmail)) {
      setEmailError("Enter a valid email address.");
      toast.error("Enter a valid email address.");
      return;
    }
    setEmailError(null);

    const next = { ...preferences };
    setSaving(true);

    try {
      const result = await updateNotificationPreferences(address, {
        email_enabled: next.email_enabled,
        notifications: next.notifications,
        ...(trimmedEmail ? { email_address: trimmedEmail } : {}),
      });
      setSavedPreferences(next);
      toast.success(
        result.verification_required
          ? "Check your inbox to verify the new email address."
          : "Notification preferences saved."
      );
    } catch (error) {
      setPreferences(savedPreferences);
      toast.error(errorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const updateNotification = (type: NotificationType, enabled: boolean) => {
    setPreferences((current) => ({
      ...current,
      notifications: { ...current.notifications, [type]: enabled },
    }));
  };

  if (!address) {
    return (
      <section className="mx-auto max-w-3xl px-4 py-12 sm:px-6" aria-labelledby="notifications-title">
        <h1 id="notifications-title" className="text-3xl font-bold text-gray-900">
          Notification preferences
        </h1>
        <div role="status" className="mt-6 rounded-xl border border-gray-200 bg-white p-6 text-sm text-gray-600">
          Connect your wallet to manage notification preferences.
        </div>
      </section>
    );
  }

  return (
    <section className="mx-auto max-w-3xl px-4 py-10 sm:px-6" aria-labelledby="notifications-title">
      <div className="mb-8">
        <p className="text-sm font-semibold uppercase tracking-wide text-blue-700">
          Account settings
        </p>
        <h1 id="notifications-title" className="mt-2 text-3xl font-bold tracking-tight text-gray-900">
          Notification preferences
        </h1>
        <p className="mt-2 text-sm text-gray-600">
          Choose which account updates you receive by email.
        </p>
      </div>

      {loadError && (
        <div role="alert" className="mb-6 rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-700">
          {loadError}
          <button type="button" onClick={() => void loadPreferences()} className="ml-3 font-semibold underline">
            Try again
          </button>
        </div>
      )}

      <form onSubmit={handleSave} noValidate className="space-y-6">
        <fieldset disabled={loading || saving} className="space-y-6">
          <div className="rounded-xl border border-gray-200 bg-white p-6 shadow-sm">
            <div className="flex items-start justify-between gap-4">
              <div>
                <h2 className="text-base font-semibold text-gray-900">Email notifications</h2>
                <p className="mt-1 text-sm text-gray-600">
                  Turn email delivery on or off for all notification types.
                </p>
              </div>
              <input
                type="checkbox"
                role="switch"
                aria-label="Enable email notifications"
                checked={preferences.email_enabled}
                onChange={(event) =>
                  setPreferences((current) => ({
                    ...current,
                    email_enabled: event.target.checked,
                  }))
                }
                className="mt-1 h-5 w-5 accent-blue-600"
              />
            </div>

            <div className="mt-6">
              <label htmlFor="notification-email" className="block text-sm font-medium text-gray-800">
                Email address
              </label>
              <input
                id="notification-email"
                type="email"
                autoComplete="email"
                value={email}
                onChange={(event) => {
                  setEmail(event.target.value);
                  if (emailError) setEmailError(null);
                }}
                aria-invalid={Boolean(emailError)}
                aria-describedby={emailError ? "notification-email-error" : "notification-email-help"}
                className="mt-2 block w-full rounded-lg border border-gray-300 px-3 py-2 text-sm text-gray-900 shadow-sm outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-100"
                placeholder="you@example.com"
              />
              {emailError ? (
                <p id="notification-email-error" role="alert" className="mt-2 text-sm text-red-600">
                  {emailError}
                </p>
              ) : (
                <p id="notification-email-help" className="mt-2 text-xs text-gray-500">
                  A new email address is activated after you verify it from your inbox.
                </p>
              )}
            </div>
          </div>

          <div className="rounded-xl border border-gray-200 bg-white p-6 shadow-sm">
            <h2 className="text-base font-semibold text-gray-900">Event updates</h2>
            <p className="mt-1 text-sm text-gray-600">
              Select the events you want to hear about.
            </p>
            <div className="mt-5 divide-y divide-gray-100">
              {NOTIFICATION_OPTIONS.map((option) => (
                <label key={option.type} className="flex cursor-pointer items-start justify-between gap-4 py-4 first:pt-0 last:pb-0">
                  <span>
                    <span className="block text-sm font-medium text-gray-900">{option.label}</span>
                    <span className="mt-1 block text-sm text-gray-500">{option.description}</span>
                  </span>
                  <input
                    type="checkbox"
                    role="switch"
                    aria-label={option.label}
                    checked={preferences.notifications[option.type]}
                    onChange={(event) => updateNotification(option.type, event.target.checked)}
                    className="mt-1 h-5 w-5 shrink-0 accent-blue-600"
                  />
                </label>
              ))}
            </div>
          </div>
        </fieldset>

        <div className="flex items-center justify-end gap-3">
          {loading && <span role="status" className="text-sm text-gray-500">Loading preferences…</span>}
          <button
            type="submit"
            disabled={loading || saving || Boolean(loadError)}
            className="rounded-lg bg-blue-600 px-5 py-2.5 text-sm font-semibold text-white shadow-sm hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {saving ? "Saving…" : "Save preferences"}
          </button>
        </div>
      </form>
    </section>
  );
}
