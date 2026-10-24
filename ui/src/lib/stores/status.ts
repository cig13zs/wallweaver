import { writable } from "svelte/store";
import { daemon } from "../daemon";
import type { DaemonStatus } from "../daemon/types";

export const status = writable<DaemonStatus | null>(null);

export async function refreshStatus(): Promise<void> {
  status.set(await (await daemon()).status());
}

export async function setPausedAll(paused: boolean): Promise<void> {
  status.set(await (await daemon()).pauseAll(paused));
}
