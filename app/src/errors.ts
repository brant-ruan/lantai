// The message to show for a failure: commands reject with a string, other failures with an Error.

export function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
