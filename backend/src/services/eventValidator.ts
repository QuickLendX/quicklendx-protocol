import { z } from "zod";
import { EventValidator, RawEvent, RawEventSchema } from "../types/replay";

const isoDateTimeSchema = z.string().datetime();
const amountSchema = z.string().min(1);

const eventEnvelopeSchema = z.object({
  id: z.string().min(1),
  ledger: z.number().int().nonnegative(),
  txHash: z.string().min(1),
  eventIndex: z.number().int().nonnegative().default(0),
  timestamp: z.number().int().nonnegative(),
  complianceHold: z.boolean().default(false),
  indexedAt: isoDateTimeSchema,
});

export const InvoiceSettledSchema = eventEnvelopeSchema.extend({
  type: z.literal("InvoiceSettled"),
  payload: z
    .object({
      invoice_id: z.string().min(1),
      business: z.string().min(1),
      investor: z.string().min(1),
      amount: amountSchema,
    })
    .passthrough(),
});

export const PaymentRecordedSchema = eventEnvelopeSchema.extend({
  type: z.literal("PaymentRecorded"),
  payload: z
    .object({
      invoice_id: z.string().min(1),
      payer: z.string().min(1),
      amount: amountSchema,
    })
    .passthrough(),
});

export const DisputeCreatedSchema = eventEnvelopeSchema.extend({
  type: z.literal("DisputeCreated"),
  payload: z
    .object({
      invoice_id: z.string().min(1),
      initiator: z.string().min(1),
    })
    .passthrough(),
});

export const DisputeResolvedSchema = eventEnvelopeSchema.extend({
  type: z.literal("DisputeResolved"),
  payload: z
    .object({
      invoice_id: z.string().min(1),
      resolved_by: z.string().min(1),
    })
    .passthrough(),
});

export const SorobanEventSchema = z.discriminatedUnion("type", [
  InvoiceSettledSchema,
  PaymentRecordedSchema,
  DisputeCreatedSchema,
  DisputeResolvedSchema,
  // Legacy/indexer events accepted by the HTTP ingestion endpoint
  eventEnvelopeSchema.extend({ type: z.literal("InvoiceCreated"), payload: z.object({}).passthrough() }),
  eventEnvelopeSchema.extend({ type: z.literal("BidPlaced"), payload: z.object({}).passthrough() }),
]);

export const SorobanEventBatchSchema = z.array(SorobanEventSchema).max(100);

export type SorobanEvent = z.infer<typeof SorobanEventSchema>;

export interface EventValidationSuccess {
  success: true;
  data: SorobanEvent;
}

export interface EventValidationFailure {
  success: false;
  errors: string[];
}

export type EventValidationResult = EventValidationSuccess | EventValidationFailure;

export interface EventBatchValidationResult {
  success: boolean;
  results: EventValidationResult[];
  errors?: string[];
}

export function validateEvent(event: unknown): EventValidationResult {
  const result = SorobanEventSchema.safeParse(event);

  if (result.success) {
    return { success: true, data: result.data };
  }

  return {
    success: false,
    errors: formatZodIssues(result.error.issues),
  };
}

export function validateEventBatch(events: unknown): EventBatchValidationResult {
  if (!Array.isArray(events)) {
    return {
      success: false,
      results: [],
      errors: ["Request body must be an event object or an array of event objects"],
    };
  }

  if (events.length > 100) {
    return {
      success: false,
      results: [],
      errors: ["Batch size exceeds 100 events"],
    };
  }

  const results = events.map((event) => validateEvent(event));

  return {
    success: results.every((result) => result.success),
    results,
  };
}

export function getStableEventId(event: SorobanEvent): string {
  return event.id;
}

function formatZodIssues(issues: z.ZodIssue[]): string[] {
  return issues.map((issue) => {
    const path = issue.path.length > 0 ? issue.path.join(".") : "event";

    if (issue.code === "invalid_union" || issue.code === "invalid_value") {
      return `${path} is not an accepted Soroban event value`;
    }

    return `${path} is invalid`;
  });
}

export class DefaultEventValidator implements EventValidator {
  private readonly maxPayloadSize: number;
  private readonly allowedEventTypes: Set<string>;
  private readonly requiredFields: Set<string>;

  constructor(
    maxPayloadSize: number = 1024 * 1024,
    allowedEventTypes?: string[]
  ) {
    this.maxPayloadSize = maxPayloadSize;
    this.allowedEventTypes = new Set(
      allowedEventTypes || [
        "InvoiceCreated",
        "InvoiceSettled",
        "BidPlaced",
        "BidAccepted",
        "PaymentRecorded",
        "DisputeCreated",
        "DisputeResolved",
        "SettlementCompleted",
      ]
    );
    this.requiredFields = new Set([
      "id",
      "ledger",
      "txHash",
      "type",
      "payload",
      "timestamp",
    ]);
  }

  async validateEvent(event: RawEvent): Promise<string[]> {
    const errors: string[] = [];
    const envelope = RawEventSchema.safeParse(event);

    if (!envelope.success) {
      errors.push(...formatZodIssues(envelope.error.issues));
    }

    for (const field of this.requiredFields) {
      if (!(field in event)) {
        errors.push(`Missing required field: ${field}`);
      }
    }

    if (typeof event.type === "string" && !this.allowedEventTypes.has(event.type)) {
      errors.push(`Event type '${event.type}' is not allowed`);
    }

    if (typeof event.payload === "object" && event.payload !== null) {
      const payloadSize = JSON.stringify(event.payload).length;
      if (payloadSize > this.maxPayloadSize) {
        errors.push(`Payload size ${payloadSize} exceeds maximum ${this.maxPayloadSize}`);
      }

      if (this.containsSuspiciousContent(JSON.stringify(event.payload))) {
        errors.push("Payload contains potentially dangerous content");
      }
    }

    return [...new Set(errors)];
  }

  async sanitizeEvent(event: RawEvent): Promise<RawEvent> {
    const sanitized = JSON.parse(JSON.stringify(event)) as RawEvent;

    sanitized.id = this.sanitizeString(sanitized.id);
    sanitized.txHash = this.sanitizeString(sanitized.txHash);
    sanitized.type = this.sanitizeString(sanitized.type);
    sanitized.indexedAt = this.sanitizeString(sanitized.indexedAt);
    sanitized.payload = this.sanitizeObject(sanitized.payload) as Record<string, unknown>;
    sanitized.complianceHold = Boolean(sanitized.complianceHold);
    sanitized.timestamp = Math.floor(Number(sanitized.timestamp));

    return sanitized;
  }

  private containsSuspiciousContent(content: string): boolean {
    const suspiciousPatterns = [
      /<script\b[^<]*(?:(?!<\/script>)<[^<]*)*<\/script>/gi,
      /javascript:/gi,
      /on\w+\s*=/gi,
      /data:text\/html/gi,
    ];

    return suspiciousPatterns.some((pattern) => pattern.test(content));
  }

  private sanitizeString(str: string): string {
    if (typeof str !== "string") return "";

    return str.replace(/[\x00-\x08\x0B\x0C\x0E-\x1F\x7F]/g, "").trim();
  }

  private sanitizeObject(obj: unknown): unknown {
    if (obj === null || typeof obj !== "object") {
      return obj;
    }

    if (Array.isArray(obj)) {
      return obj.map((item) => this.sanitizeObject(item));
    }

    const sanitized: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(obj)) {
      const sanitizedKey = this.sanitizeString(key);

      if (typeof value === "string") {
        sanitized[sanitizedKey] = this.sanitizeString(value);
      } else if (typeof value === "object" && value !== null) {
        sanitized[sanitizedKey] = this.sanitizeObject(value);
      } else {
        sanitized[sanitizedKey] = value;
      }
    }

    return sanitized;
  }
}
