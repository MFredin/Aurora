import { createHmac, timingSafeEqual } from "node:crypto";

/** Verify `X-Hub-Signature-256` against the raw request body. Call before parsing JSON. */
export function verifySignature(secret: string, rawBody: Buffer | string, header: string | undefined): boolean {
  if (!header || !header.startsWith("sha256=")) return false;
  const expected = Buffer.from(`sha256=${createHmac("sha256", secret).update(rawBody).digest("hex")}`);
  const given = Buffer.from(header);
  return given.length === expected.length && timingSafeEqual(given, expected);
}
