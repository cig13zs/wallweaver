import { describe, expect, it } from "vitest";
import { artKey, LruCache } from "./cache";

describe("artKey", () => {
  it("separates every input that changes the picture", () => {
    const base = artKey({ kind: "aurora", seed: 4 }, 200, 100, 1);
    expect(artKey({ kind: "aurora", seed: 4 }, 200, 100, 1)).toBe(base);
    expect(artKey({ kind: "aurora", seed: 5 }, 200, 100, 1)).not.toBe(base);
    expect(artKey({ kind: "aurora", seed: 4 }, 200, 100, 2)).not.toBe(base);
    expect(artKey({ kind: "aurora", seed: 4 }, 201, 100, 1)).not.toBe(base);
    expect(artKey({ kind: "aurora", seed: 4, variant: "light" }, 200, 100, 1)).not.toBe(base);
    expect(artKey({ kind: "aurora", seed: 4, pal: ["#000000"] }, 200, 100, 1)).not.toBe(base);
  });
});

describe("LruCache", () => {
  it("evicts the least recently used entry", () => {
    const c = new LruCache<number>(2);
    c.set("a", 1);
    c.set("b", 2);
    expect(c.get("a")).toBe(1);
    c.set("c", 3);
    expect(c.get("b")).toBeUndefined();
    expect(c.get("a")).toBe(1);
    expect(c.get("c")).toBe(3);
    expect(c.size).toBe(2);
  });
  it("replaces an existing key without growing", () => {
    const c = new LruCache<number>(2);
    c.set("a", 1);
    c.set("a", 2);
    expect(c.size).toBe(1);
    expect(c.get("a")).toBe(2);
  });
});
