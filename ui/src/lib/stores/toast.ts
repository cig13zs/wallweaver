import { writable } from "svelte/store";

export interface Toast { id: number; text: string; tone: "ok" | "info" | "bad" }

export const toasts = writable<Toast[]>([]);
let next = 1;

/** Shows a short message in the polite live region; it clears itself. */
export function toast(text: string, tone: Toast["tone"] = "ok", ms = 3200): void {
  const id = next++;
  toasts.update((t) => [...t.slice(-2), { id, text, tone }]);
  setTimeout(() => toasts.update((t) => t.filter((x) => x.id !== id)), ms);
}

/** Wraps an async UI action so failures become a visible message instead of a silent rejection. */
export async function attempt<T>(fn: () => Promise<T>, failure = "That didn't work"): Promise<T | undefined> {
  try {
    return await fn();
  } catch (e) {
    toast(`${failure}: ${e instanceof Error ? e.message : String(e)}`, "bad", 5000);
    return undefined;
  }
}
