import { z } from "zod";
import { apiClient } from "@/app/lib/api-client";

/** Values mirror backend/src/types/contract.ts. */
export const NotificationType = {
  InvoiceFunded: "invoice_funded",
  PaymentReceived: "payment_received",
  DisputeOpened: "dispute_opened",
  DisputeResolved: "dispute_resolved",
} as const;

export type NotificationType =
  (typeof NotificationType)[keyof typeof NotificationType];

const notificationsSchema = z.object({
  [NotificationType.InvoiceFunded]: z.boolean(),
  [NotificationType.PaymentReceived]: z.boolean(),
  [NotificationType.DisputeOpened]: z.boolean(),
  [NotificationType.DisputeResolved]: z.boolean(),
});

const preferencesSchema = z.object({
  email_enabled: z.boolean(),
  email_address: z.string().email().optional(),
  notifications: notificationsSchema,
});

const updatePreferencesSchema = z.object({
  email_enabled: z.boolean().optional(),
  email_address: z.string().email().optional(),
  notifications: notificationsSchema.partial().optional(),
});

const updateResponseSchema = z.union([
  z.object({ success: z.literal(true) }),
  z.object({ verification_required: z.literal(true) }),
]);

export type NotificationPreferences = z.infer<typeof preferencesSchema>;
export type UpdateNotificationPreferences = z.infer<
  typeof updatePreferencesSchema
>;
export type UpdateNotificationPreferencesResult = z.infer<
  typeof updateResponseSchema
>;

function preferencesUrl(userId: string): string {
  return `/notifications/preferences/${encodeURIComponent(userId)}`;
}

export async function getNotificationPreferences(
  userId: string
): Promise<NotificationPreferences> {
  const response = await apiClient.get<unknown>(preferencesUrl(userId));
  return preferencesSchema.parse(response);
}

export async function updateNotificationPreferences(
  userId: string,
  updates: UpdateNotificationPreferences
): Promise<UpdateNotificationPreferencesResult> {
  const payload = updatePreferencesSchema.parse(updates);
  const response = await apiClient.put<unknown>(
    preferencesUrl(userId),
    payload,
    { retry: false }
  );
  return updateResponseSchema.parse(response);
}
