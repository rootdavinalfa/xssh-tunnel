import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

// Type-safe wrapper for the greet command
export async function greet(name: string): Promise<string> {
  return await invoke('greet', { name });
}

// Generic typed event listener helper
export function onEvent<T>(eventName: string, callback: (payload: T) => void) {
  return listen<T>(eventName, (event) => callback(event.payload));
}