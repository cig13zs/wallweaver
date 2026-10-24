import type { Daemon } from "./types";

export type { Daemon } from "./types";

/** True inside the desktop shell; false in a plain browser tab. */
export function inShell(w: object = globalThis): boolean {
  return "__TAURI_INTERNALS__" in w;
}

let current: Promise<Daemon> | null = null;

/** The daemon client for this window. Both implementations load lazily so neither weighs on the first paint. */
export function daemon(): Promise<Daemon> {
  current ??= inShell()
    ? import("./tauri").then((m) => new m.TauriDaemon())
    : import("./mock").then((m) => new m.MockDaemon({ live: !new URLSearchParams(location.search).has("static") }));
  return current;
}

/** Test hook: swap in a specific client. */
export function setDaemon(d: Daemon): void {
  current = Promise.resolve(d);
}
