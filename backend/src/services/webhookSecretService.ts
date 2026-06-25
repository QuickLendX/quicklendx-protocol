import {
  createHmac,
  randomBytes,
  timingSafeEqual,
  generateKeyPairSync,
  createPrivateKey,
  createPublicKey,
  sign,
  verify,
} from "crypto";
import {
  WebhookSubscriberSecret,
  WebhookSecretStatus,
  WebhookVerificationResult,
  SubscriberSecretPublicView,
  InitiateRotationResponse,
  FinalizeRotationResponse,
} from "../types/webhook";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/** Length of generated secrets in bytes (256-bit entropy). */
const SECRET_BYTE_LENGTH = 32;

/** HMAC algorithm used for webhook signature computation. */
const HMAC_ALGORITHM = "sha256";

/** Prefix used in the X-Webhook-Signature header value. */
const SIGNATURE_PREFIX = "sha256=";

/** Regex for validating base64url Ed25519 signatures (exactly 86 characters). */
const BASE64URL_REGEX = /^[A-Za-z0-9_-]{86}$/;

/** Pre-generated static dummy keypair for timing-safe dummy Ed25519 checks. */
const DUMMY_KEY_PAIR = generateKeyPairSync("ed25519");
const DUMMY_PUBLIC_KEY = DUMMY_KEY_PAIR.publicKey.export({ type: "spki", format: "pem" }) as string;
const DUMMY_SIGNATURE = Buffer.alloc(64);

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

export class WebhookSecretError extends Error {
  public readonly code: string;
  public readonly status: number;

  constructor(message: string, code: string, status: number) {
    super(message);
    this.name = "WebhookSecretError";
    this.code = code;
    this.status = status;
  }
}

// ---------------------------------------------------------------------------
// In-memory store (replace with a persistent DB adapter in production)
// ---------------------------------------------------------------------------

/**
 * Simple in-memory store keyed by subscriber_id.
 * In production this would be backed by a database with encrypted secret
 * columns and row-level locking to prevent concurrent rotation races.
 */
export class WebhookSecretStore {
  private readonly store = new Map<string, WebhookSubscriberSecret>();

  get(subscriberId: string): WebhookSubscriberSecret | undefined {
    return this.store.get(subscriberId);
  }

  set(record: WebhookSubscriberSecret): void {
    this.store.set(record.subscriber_id, record);
  }

  delete(subscriberId: string): boolean {
    return this.store.delete(subscriberId);
  }

  has(subscriberId: string): boolean {
    return this.store.has(subscriberId);
  }

  /** Exposed for testing only – clears all records. */
  _clear(): void {
    this.store.clear();
  }

  /** Exposed for testing only – returns all records. */
  _all(): WebhookSubscriberSecret[] {
    return Array.from(this.store.values());
  }
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

export class WebhookSecretService {
  private static instance: WebhookSecretService;

  constructor(private readonly store: WebhookSecretStore = new WebhookSecretStore()) {}

  // -------------------------------------------------------------------------
  // Singleton
  // -------------------------------------------------------------------------

  public static getInstance(): WebhookSecretService {
    if (!WebhookSecretService.instance) {
      WebhookSecretService.instance = new WebhookSecretService();
    }
    return WebhookSecretService.instance;
  }

  // -------------------------------------------------------------------------
  // Secret generation
  // -------------------------------------------------------------------------

  /**
   * Generates a cryptographically secure random secret.
   * Returns a hex-encoded string (64 characters for 32 bytes).
   */
  public generateSecret(): string {
    return randomBytes(SECRET_BYTE_LENGTH).toString("hex");
  }

  // -------------------------------------------------------------------------
  // Subscriber management
  // -------------------------------------------------------------------------

  /**
   * Registers a new subscriber and generates their initial secret.
   * Returns the public view (no secret) plus the initial secret value
   * which must be delivered to the subscriber out-of-band.
   */
  public registerSubscriber(
    subscriberId: string,
    gracePeriodSeconds: number = 3600,
    algorithm: "hmac-sha256" | "ed25519" = "hmac-sha256"
  ): { view: SubscriberSecretPublicView; initial_secret: string } {
    if (this.store.has(subscriberId)) {
      throw new WebhookSecretError(
        "Subscriber already registered",
        "SUBSCRIBER_ALREADY_EXISTS",
        409
      );
    }

    const now = new Date().toISOString();
    let primarySecret: string;
    let initialSecret: string;

    if (algorithm === "ed25519") {
      const keys = this.generateEd25519KeyPair();
      primarySecret = keys.privateKey;
      initialSecret = keys.publicKey;
    } else {
      primarySecret = this.generateSecret();
      initialSecret = primarySecret;
    }

    const record: WebhookSubscriberSecret = {
      subscriber_id: subscriberId,
      primary_secret: primarySecret,
      pending_secret: null,
      pending_created_at: null,
      grace_period_seconds: gracePeriodSeconds,
      status: WebhookSecretStatus.Active,
      algorithm,
      created_at: now,
      updated_at: now,
    };

    this.store.set(record);

    return {
      view: this.toPublicView(record),
      initial_secret: initialSecret,
    };
  }

  /**
   * Returns the public (non-secret) view of a subscriber's secret state.
   */
  public getSubscriberView(subscriberId: string): SubscriberSecretPublicView {
    const record = this.requireRecord(subscriberId);
    return this.toPublicView(record);
  }

  // -------------------------------------------------------------------------
  // Rotation lifecycle
  // -------------------------------------------------------------------------

  /**
   * Step 1 – Initiate rotation.
   *
   * Generates a new pending secret and enters the dual-verify window.
   * Both the existing primary and the new pending secret will be accepted
   * for signature verification until the grace period expires or rotation
   * is finalized.
   *
   * The new secret is returned **once** in the response.  The caller must
   * store it securely; it will never be returned again.
   */
  public initiateRotation(
    subscriberId: string,
    gracePeriodSeconds?: number
  ): InitiateRotationResponse {
    const record = this.requireRecord(subscriberId);

    if (record.status === WebhookSecretStatus.Rotating) {
      throw new WebhookSecretError(
        "A rotation is already in progress for this subscriber. " +
          "Finalize or cancel the existing rotation before starting a new one.",
        "ROTATION_ALREADY_IN_PROGRESS",
        409
      );
    }

    const now = new Date().toISOString();
    let newSecret: string;
    let returnedSecret: string;

    if (record.algorithm === "ed25519") {
      const keys = this.generateEd25519KeyPair();
      newSecret = keys.privateKey;
      returnedSecret = keys.publicKey;
    } else {
      newSecret = this.generateSecret();
      returnedSecret = newSecret;
    }
    const effectiveGrace = gracePeriodSeconds ?? record.grace_period_seconds;

    const updated: WebhookSubscriberSecret = {
      ...record,
      pending_secret: newSecret,
      pending_created_at: now,
      grace_period_seconds: effectiveGrace,
      status: WebhookSecretStatus.Rotating,
      updated_at: now,
    };

    this.store.set(updated);

    return {
      subscriber_id: subscriberId,
      status: WebhookSecretStatus.Rotating,
      new_secret: returnedSecret,
      grace_period_seconds: effectiveGrace,
      pending_created_at: now,
    };
  }

  /**
   * Step 2 – Finalize rotation.
   *
   * Promotes the pending secret to primary and clears the old secret.
   * After this call only the new secret is accepted for verification.
   */
  public finalizeRotation(subscriberId: string): FinalizeRotationResponse {
    const record = this.requireRecord(subscriberId);

    if (record.status !== WebhookSecretStatus.Rotating) {
      throw new WebhookSecretError(
        "No rotation is in progress for this subscriber.",
        "NO_ROTATION_IN_PROGRESS",
        409
      );
    }

    if (!record.pending_secret) {
      // Defensive: should not happen given status check above.
      throw new WebhookSecretError(
        "Rotation state is inconsistent: status is rotating but no pending secret found.",
        "ROTATION_STATE_INCONSISTENT",
        500
      );
    }

    const now = new Date().toISOString();

    const updated: WebhookSubscriberSecret = {
      ...record,
      primary_secret: record.pending_secret,
      pending_secret: null,
      pending_created_at: null,
      status: WebhookSecretStatus.Active,
      updated_at: now,
    };

    this.store.set(updated);

    return {
      subscriber_id: subscriberId,
      status: WebhookSecretStatus.Active,
      message:
        "Rotation finalized. The new secret is now the only accepted signing key.",
    };
  }

  /**
   * Cancel an in-progress rotation, reverting to the primary secret only.
   */
  public cancelRotation(subscriberId: string): SubscriberSecretPublicView {
    const record = this.requireRecord(subscriberId);

    if (record.status !== WebhookSecretStatus.Rotating) {
      throw new WebhookSecretError(
        "No rotation is in progress for this subscriber.",
        "NO_ROTATION_IN_PROGRESS",
        409
      );
    }

    const now = new Date().toISOString();

    const updated: WebhookSubscriberSecret = {
      ...record,
      pending_secret: null,
      pending_created_at: null,
      status: WebhookSecretStatus.Active,
      updated_at: now,
    };

    this.store.set(updated);
    return this.toPublicView(updated);
  }

  // -------------------------------------------------------------------------
  // Signature computation & verification
  // -------------------------------------------------------------------------

  /**
   * Computes an HMAC-SHA256 signature for the given payload using the
   * provided secret.
   *
   * @param payload  Raw request body as a Buffer or string.
   * @param secret   Hex-encoded secret.
   * @returns        Signature string in the format `sha256=<hex>`.
   */
  public computeSignature(payload: Buffer | string, secret: string): string {
    const hmac = createHmac(HMAC_ALGORITHM, Buffer.from(secret, "hex"));
    hmac.update(typeof payload === "string" ? Buffer.from(payload) : payload);
    return `${SIGNATURE_PREFIX}${hmac.digest("hex")}`;
  }

  /**
   * Verifies an incoming webhook signature against a subscriber's secrets.
   *
   * During a rotation window both the primary and pending secrets are tried.
   * Uses `timingSafeEqual` to prevent timing-based secret oracle attacks.
   *
   * @param subscriberId  The subscriber whose secrets to check.
   * @param payload       Raw request body (must be the exact bytes received).
   * @param signature     Value of the `X-Webhook-Signature` header.
   * @returns             Verification result including which secret matched.
   */
  public verifySignature(
    subscriberId: string,
    payload: Buffer | string,
    signature: string,
    negotiatedAlgorithm: "hmac-sha256" | "ed25519" = "hmac-sha256"
  ): WebhookVerificationResult {
    const record = this.requireRecord(subscriberId);

    // Auto-expire pending secret if grace period has elapsed.
    const effectiveRecord = this.maybeExpirePending(record);

    const invalid: WebhookVerificationResult = {
      valid: false,
      matched_secret: null,
    };

    // If there is a mismatch between database algorithm and negotiated algorithm, it must fail.
    if (effectiveRecord.algorithm !== negotiatedAlgorithm) {
      if (negotiatedAlgorithm === "ed25519") {
        this.dummyVerifyEd25519(payload);
      } else {
        this.dummyVerifyHmac(payload);
      }
      return invalid;
    }

    if (negotiatedAlgorithm === "hmac-sha256") {
      if (!signature || !signature.startsWith(SIGNATURE_PREFIX)) {
        return invalid;
      }

      const incomingSig = Buffer.from(signature);

      // Check primary secret.
      const primarySig = Buffer.from(
        this.computeSignature(payload, effectiveRecord.primary_secret)
      );
      if (
        incomingSig.length === primarySig.length &&
        timingSafeEqual(incomingSig, primarySig)
      ) {
        return { valid: true, matched_secret: "primary" };
      }

      // Check pending secret (only during rotation window).
      if (
        effectiveRecord.status === WebhookSecretStatus.Rotating &&
        effectiveRecord.pending_secret
      ) {
        const pendingSig = Buffer.from(
          this.computeSignature(payload, effectiveRecord.pending_secret)
        );
        if (
          incomingSig.length === pendingSig.length &&
          timingSafeEqual(incomingSig, pendingSig)
        ) {
          return { valid: true, matched_secret: "pending" };
        }
      }
    } else if (negotiatedAlgorithm === "ed25519") {
      // Validate that signature is a valid base64url string of 86 chars
      if (!signature || !BASE64URL_REGEX.test(signature)) {
        this.dummyVerifyEd25519(payload);
        return invalid;
      }

      // Verify against primary key (which stores the private key PEM)
      const primaryValid = this.verifyEd25519(
        payload,
        signature,
        effectiveRecord.primary_secret
      );
      if (primaryValid) {
        return { valid: true, matched_secret: "primary" };
      }

      // Verify against pending key (only during rotation window)
      if (
        effectiveRecord.status === WebhookSecretStatus.Rotating &&
        effectiveRecord.pending_secret
      ) {
        const pendingValid = this.verifyEd25519(
          payload,
          signature,
          effectiveRecord.pending_secret
        );
        if (pendingValid) {
          return { valid: true, matched_secret: "pending" };
        }
      }
    }

    return invalid;
  }

  // -------------------------------------------------------------------------
  // Private helpers
  // -------------------------------------------------------------------------

  private requireRecord(subscriberId: string): WebhookSubscriberSecret {
    const record = this.store.get(subscriberId);
    if (!record) {
      throw new WebhookSecretError(
        "Subscriber not found",
        "SUBSCRIBER_NOT_FOUND",
        404
      );
    }
    return record;
  }

  /**
   * If the pending secret's grace period has elapsed, automatically promote
   * the pending secret to primary (lazy expiry).  This prevents a stale
   * pending secret from being accepted indefinitely if `finalizeRotation`
   * is never called.
   */
  private maybeExpirePending(
    record: WebhookSubscriberSecret
  ): WebhookSubscriberSecret {
    if (
      record.status !== WebhookSecretStatus.Rotating ||
      !record.pending_secret ||
      !record.pending_created_at
    ) {
      return record;
    }

    const pendingAge =
      (Date.now() - new Date(record.pending_created_at).getTime()) / 1000;

    if (pendingAge >= record.grace_period_seconds) {
      // Grace period elapsed: auto-promote pending → primary.
      const now = new Date().toISOString();
      const promoted: WebhookSubscriberSecret = {
        ...record,
        primary_secret: record.pending_secret,
        pending_secret: null,
        pending_created_at: null,
        status: WebhookSecretStatus.Active,
        updated_at: now,
      };
      this.store.set(promoted);
      return promoted;
    }

    return record;
  }

  private toPublicView(
    record: WebhookSubscriberSecret
  ): SubscriberSecretPublicView {
    return {
      subscriber_id: record.subscriber_id,
      status: record.status,
      has_pending_secret: record.pending_secret !== null,
      pending_created_at: record.pending_created_at,
      grace_period_seconds: record.grace_period_seconds,
      algorithm: record.algorithm,
      created_at: record.created_at,
      updated_at: record.updated_at,
    };
  }

  // -------------------------------------------------------------------------
  // Ed25519 & JWKS helpers
  // -------------------------------------------------------------------------

  /**
   * Generates a new Ed25519 private/public key pair in PEM format.
   */
  public generateEd25519KeyPair(): { privateKey: string; publicKey: string } {
    const { publicKey, privateKey } = generateKeyPairSync("ed25519");
    return {
      privateKey: privateKey.export({ type: "pkcs8", format: "pem" }) as string,
      publicKey: publicKey.export({ type: "spki", format: "pem" }) as string,
    };
  }

  /**
   * Signs a payload using an Ed25519 private key in PEM format.
   * Returns a base64url-encoded signature.
   */
  public signEd25519(payload: Buffer | string, privateKeyPem: string): string {
    const privateKey = createPrivateKey(privateKeyPem);
    const data = typeof payload === "string" ? Buffer.from(payload) : payload;
    const signature = sign(null, data, privateKey);
    return signature.toString("base64url");
  }

  /**
   * Verifies an Ed25519 signature (base64url-encoded) against a PEM private key
   * by extracting the public key.
   */
  public verifyEd25519(
    payload: Buffer | string,
    signatureBase64Url: string,
    privateKeyPem: string
  ): boolean {
    try {
      const privateKey = createPrivateKey(privateKeyPem);
      const publicKey = createPublicKey(privateKey);
      const data = typeof payload === "string" ? Buffer.from(payload) : payload;
      const sigBuf = Buffer.from(signatureBase64Url, "base64url");
      return verify(null, data, publicKey, sigBuf);
    } catch {
      return false;
    }
  }

  /**
   * Runs a dummy Ed25519 verification to match execution time (timing-safe).
   */
  public dummyVerifyEd25519(payload: Buffer | string): boolean {
    try {
      const data = typeof payload === "string" ? Buffer.from(payload) : payload;
      return verify(null, data, DUMMY_PUBLIC_KEY, DUMMY_SIGNATURE);
    } catch {
      return false;
    }
  }

  /**
   * Runs a dummy HMAC verification to match execution time (timing-safe).
   */
  public dummyVerifyHmac(payload: Buffer | string): boolean {
    const dummyHmac = createHmac("sha256", Buffer.from("dummy-secret"));
    dummyHmac.update(typeof payload === "string" ? Buffer.from(payload) : payload);
    const dummySig = dummyHmac.digest();
    return timingSafeEqual(Buffer.alloc(dummySig.length), dummySig);
  }

  /**
   * Exposes the active public key set in JWKS-style format.
   */
  public getActiveJWKs(): any[] {
    const records = this.store._all();
    const keys: any[] = [];

    for (const record of records) {
      if (record.algorithm !== "ed25519") {
        continue;
      }

      // Export primary key
      try {
        const privateKey = createPrivateKey(record.primary_secret);
        const publicKey = createPublicKey(privateKey);
        const jwk = publicKey.export({ format: "jwk" });
        keys.push({
          kty: jwk.kty,
          crv: jwk.crv,
          x: jwk.x,
          kid: record.subscriber_id,
          use: "sig",
          alg: "EdDSA",
        });
      } catch (err) {
        // Ignore malformed keys
      }

      // Export pending key if rotating
      const effectiveRecord = this.maybeExpirePending(record);
      if (
        effectiveRecord.status === WebhookSecretStatus.Rotating &&
        effectiveRecord.pending_secret
      ) {
        try {
          const privateKey = createPrivateKey(effectiveRecord.pending_secret);
          const publicKey = createPublicKey(privateKey);
          const jwk = publicKey.export({ format: "jwk" });
          keys.push({
            kty: jwk.kty,
            crv: jwk.crv,
            x: jwk.x,
            kid: `${effectiveRecord.subscriber_id}:pending`,
            use: "sig",
            alg: "EdDSA",
          });
        } catch (err) {
          // Ignore malformed keys
        }
      }
    }

    return keys;
  }
}

// Singleton export for use in controllers / middleware.
export const webhookSecretService = WebhookSecretService.getInstance();
