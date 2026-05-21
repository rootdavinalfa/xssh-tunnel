import { writable } from 'svelte/store';

export interface HelperStatus {
  installed: boolean;
  running: boolean;
}

export const helperStatus = writable<HelperStatus>({ installed: false, running: false });