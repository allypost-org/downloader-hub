/**
 * Generate a cryptographically random delivery attempt id.
 *
 * Used as the fence token for the `delivering` lease: only the attempt that
 * produced a given id may finish, release, or clean up that lease. Must be
 * unpredictable and unique enough that two concurrent acks cannot collide.
 */
export function randomDeliveryAttemptId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join(
    "",
  );
  return hex;
}
