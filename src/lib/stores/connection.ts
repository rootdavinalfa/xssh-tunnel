import { writable } from 'svelte/store';

export type ConnectionState =
  | 'disconnected'
  | 'connecting'
  | 'authenticating'
  | 'tunnel-active'
  | 'reconnecting'
  | 'error';

export const connectionState = writable<ConnectionState>('disconnected');
export const connectionError = writable<string>('');

export interface StatsSnapshot {
  bytes_up: number;
  bytes_down: number;
  uptime_secs: number;
}

export const connectionStats = writable<StatsSnapshot | null>(null);