import { afterEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";
import { attempt, toast, toasts } from "./toast";

describe("toasts", () => {
  afterEach(() => { vi.useRealTimers(); toasts.set([]); });

  it("keeps at most three and clears itself", () => {
    vi.useFakeTimers();
    for (const t of ["a", "b", "c", "d"]) toast(t);
    expect(get(toasts).map((t) => t.text)).toEqual(["b", "c", "d"]);
    vi.advanceTimersByTime(4000);
    expect(get(toasts)).toEqual([]);
  });

  it("turns a failed action into a visible message", async () => {
    const out = await attempt(async () => { throw new Error("daemon offline"); }, "Couldn't apply");
    expect(out).toBeUndefined();
    expect(get(toasts).at(-1)).toMatchObject({ text: "Couldn't apply: daemon offline", tone: "bad" });
  });

  it("passes results through", async () => {
    expect(await attempt(async () => 42)).toBe(42);
  });
});
