/**
 * Evidence file validation for the dispute response composer.
 *
 * Validation is based on content sniffing (magic bytes), not on the
 * user-controlled file name or declared MIME type. The allow-list is PDF,
 * PNG and JPEG as per docs/ux/dispute-response-composer.md.
 */

export const MAX_FILE_SIZE = 10 * 1024 * 1024 // 10 MiB per file
export const MAX_FILE_COUNT = 5
export const MAX_TOTAL_SIZE = 25 * 1024 * 1024 // 25 MiB in total

export type RejectionReason =
  | "type"
  | "size"
  | "total"
  | "empty"
  | "count"
  | "duplicate"

export interface RejectedFile {
  name: string;
  reason: RejectionReason;
}

export interface ValidateEvidenceResult {
  accepted: File[];
  rejected: RejectedFile[];
}

const PDF_SIGNATURE = [0x25, 0x50, 0x44, 0x46, 0x2d] // "%PDF-"
const PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]
const JPEG_SIGNATURE = [0xff, 0xd8, 0xff]

function startsWith(bytes: Uint8Array, signature: number[]): boolean {
  if (bytes.length < signature.length) return false;
  for (let i = 0; i < signature.length; i++) {
    if (bytes[i] !== signature[i]) return false;
  }
  return true;
}

async function hasAllowedSignature(file: File): Promise<boolean> {
  const buffer = await file.slice(0, 8).arrayBuffer();
  const bytes = new Uint8Array(buffer);
  return (
    startsWith(bytes, PDF_SIGNATURE) ||
    startsWith(bytes, PNG_SIGNATURE) ||
    startsWith(bytes, JPEG_SIGNATURE)
  );
}

function isDuplicate(file: File, others: File[]): boolean {
  return others.some(f => f.name === file.name && f.size === file.size);
}

export async function validateEvidenceFiles(
  files: File[],
  existing: File[] = [],
): Promise<ValidateEvidenceResult> {
  const accepted: File[] = [];
  const rejected: RejectedFile[] = [];
  const known: File[] = [...existing];
  let totalSize = existing.reduce((sum, f) => sum + f.size, 0);

  for (const file of files) {
    if (known.length >= MAX_FILE_COUNT) {
      rejected.push({ name: file.name, reason: "count" });
      continue;
    }

    if (file.size === 0) {
      rejected.push({ name: file.name, reason: "empty" });
      continue;
    }

    if (file.size > MAX_FILE_SIZE) {
      rejected.push({ name: file.name, reason: "size" });
      continue;
    }

    if (isDuplicate(file, known)) {
      rejected.push({ name: file.name, reason: "duplicate" });
      continue;
    }

    if (!(await hasAllowedSignature(file))) {
      rejected.push({ name: file.name, reason: "type" });
      continue;
    }

    if (totalSize + file.size > MAX_TOTAL_SIZE) {
      rejected.push({ name: file.name, reason: "total" });
      continue;
    }

    accepted.push(file);
    known.push(file);
    totalSize += file.size;
  }

  return { accepted, rejected };
}

const BIDI_CONTROLS = /[\u202A-\u202E\u2066-\u2069]/g;
// Control characters (C0) and DEL (C1), excluding tab/newline/carriage return.
const CONTROL_CHARS = /[\u0000-\u0008\u000B-\u001F\u007F-\u009F]/g;

const MAX_DISPLAY_NAME = 40;

/**
 * Strips bidi control characters and other control characters from a
 * user-provided file name and middle-truncates it to 40 characters.
 */
export function displayFileName(name: string): string {
  const sanitized = name.replace(BIDI_CONTROLS, '').replace(CONTROL_CHARS, '');
  if (sanitized.length <= MAX_DISPLAY_NAME) return sanitized;

  const keep = MAX_DISPLAY_NAME - 1; // reserve one char for the ellipsis
  const headLen = Math.ceil(keep / 2);
  const tailLen = keep - headLen;
  return `${sanitized.slice(0, headLen)}…${sanitized.slice(sanitized.length - tailLen)}`;
}
