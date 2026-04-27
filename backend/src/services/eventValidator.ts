import { RawEvent, EventValidator } from "../types/replay";

export class DefaultEventValidator implements EventValidator {
  private readonly maxPayloadSize: number;
  private readonly allowedEventTypes: Set<string>;
  private readonly requiredFields: Set<string>;

  constructor(
    maxPayloadSize: number = 1024 * 1024, // 1MB
    allowedEventTypes?: string[]
  ) {
    this.maxPayloadSize = maxPayloadSize;
    this.allowedEventTypes = new Set(allowedEventTypes || [
      "InvoiceCreated",
      "InvoiceSettled", 
      "BidPlaced",
      "BidAccepted",
      "PaymentRecorded",
      "DisputeCreated",
      "DisputeResolved",
      "SettlementCompleted"
    ]);
    
    this.requiredFields = new Set(["id", "ledger", "txHash", "type", "payload", "timestamp"]);
  }

  async validateEvent(event: RawEvent): Promise<string[]> {
    const errors: string[] = [];

    // Check required fields
    for (const field of this.requiredFields) {
      if (!(field in event)) {
        errors.push(`Missing required field: ${field}`);
      }
    }

    // Validate field types and values
    if (typeof event.id !== "string" || event.id.length === 0) {
      errors.push("Event ID must be a non-empty string");
    }

    if (typeof event.ledger !== "number" || event.ledger < 0 || !Number.isInteger(event.ledger)) {
      errors.push("Ledger must be a non-negative integer");
    }

    if (typeof event.txHash !== "string" || event.txHash.length === 0) {
      errors.push("Transaction hash must be a non-empty string");
    }

    if (typeof event.type !== "string" || event.type.length === 0) {
      errors.push("Event type must be a non-empty string");
    } else if (!this.allowedEventTypes.has(event.type)) {
      errors.push(`Event type '${event.type}' is not allowed`);
    }

    if (typeof event.timestamp !== "number" || event.timestamp < 0 || !Number.isInteger(event.timestamp)) {
      errors.push("Timestamp must be a non-negative integer");
    }

    if (typeof event.complianceHold !== "boolean") {
      errors.push("Compliance hold must be a boolean");
    }

    if (typeof event.indexedAt !== "string" || !this.isValidISODate(event.indexedAt)) {
      errors.push("Indexed at must be a valid ISO 8601 date string");
    }

    // Validate payload
    if (typeof event.payload !== "object" || event.payload === null) {
      errors.push("Payload must be an object");
    } else {
      const payloadSize = JSON.stringify(event.payload).length;
      if (payloadSize > this.maxPayloadSize) {
        errors.push(`Payload size ${payloadSize} exceeds maximum ${this.maxPayloadSize}`);
      }

      // Check for potentially dangerous content
      const payloadStr = JSON.stringify(event.payload);
      if (this.containsSuspiciousContent(payloadStr)) {
        errors.push("Payload contains potentially dangerous content");
      }
    }

    return errors;
  }

  async sanitizeEvent(event: RawEvent): Promise<RawEvent> {
    // Create a deep copy to avoid mutating the original
    const sanitized = JSON.parse(JSON.stringify(event)) as RawEvent;

    // Sanitize string fields
    sanitized.id = this.sanitizeString(sanitized.id);
    sanitized.txHash = this.sanitizeString(sanitized.txHash);
    sanitized.type = this.sanitizeString(sanitized.type);
    sanitized.indexedAt = this.sanitizeString(sanitized.indexedAt);

    // Sanitize payload recursively
    sanitized.payload = this.sanitizeObject(sanitized.payload);

    // Ensure compliance hold is boolean
    sanitized.complianceHold = Boolean(sanitized.complianceHold);

    // Round timestamp to integer
    sanitized.timestamp = Math.floor(Number(sanitized.timestamp));

    return sanitized;
  }

  private isValidISODate(dateString: string): boolean {
    const date = new Date(dateString);
    return !isNaN(date.getTime()) && dateString === date.toISOString();
  }

  private containsSuspiciousContent(content: string): boolean {
    // Check for common attack patterns
    const suspiciousPatterns = [
      /<script\b[^<]*(?:(?!<\/script>)<[^<]*)*<\/script>/gi, // Script tags
      /javascript:/gi, // JavaScript protocol
      /on\w+\s*=/gi, // Event handlers
      /data:text\/html/gi, // Data URLs
    ];

    return suspiciousPatterns.some(pattern => pattern.test(content));
  }

  private sanitizeString(str: string): string {
    if (typeof str !== "string") return "";
    
    // Remove null bytes and control characters except newlines and tabs
    return str
      .replace(/[\x00-\x08\x0B\x0C\x0E-\x1F\x7F]/g, "")
      .trim();
  }

  private sanitizeObject(obj: any): any {
    if (obj === null || typeof obj !== "object") {
      return obj;
    }

    if (Array.isArray(obj)) {
      return obj.map(item => this.sanitizeObject(item));
    }

    const sanitized: any = {};
    for (const [key, value] of Object.entries(obj)) {
      // Sanitize key
      const sanitizedKey = this.sanitizeString(key);
      
      // Sanitize value based on type
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
