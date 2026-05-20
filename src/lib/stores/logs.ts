import { writable } from 'svelte/store';

export interface LogEntry {
  id: string;
  timestamp: string;
  level: 'info' | 'warn' | 'error' | 'debug';
  message: string;
  profile_id: string | null;
}

export const logEntries = writable<LogEntry[]>([]);

export function appendLog(entry: LogEntry) {
  logEntries.update(entries => {
    const updated = [entry, ...entries];
    return updated.slice(0, 500);
  });
}

export function clearStore() {
  logEntries.set([]);
}