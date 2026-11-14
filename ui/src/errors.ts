/** Turn process, IPC and core errors into concise messages suitable for UI. */
export function userError(reason: unknown, fallback = "Something went wrong. Please try again."): string {
  let message = reason instanceof Error ? reason.message : String(reason);
  for (let pass = 0; pass < 3; pass++) {
    message = message.replace(/^Error invoking remote method '[^']+':\s*/i, "");
    message = message.replace(/^Error occurred in handler for '[^']+':\s*/i, "");
    message = message.replace(/^Error:\s*/i, "");
  }
  message = message.trim();
  return message || fallback;
}
