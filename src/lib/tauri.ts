import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { connectionState, connectionError } from './stores/connection';

// Type-safe wrapper for the greet command
export async function greet(name: string): Promise<string> {
  return await invoke('greet', { name });
}

// Generic typed event listener helper
export function onEvent<T>(eventName: string, callback: (payload: T) => void) {
  return listen<T>(eventName, (event) => callback(event.payload));
}

export async function connectTunnel(): Promise<string> {
  return await invoke('connect_tunnel');
}

export async function disconnectTunnel(): Promise<string> {
  return await invoke('disconnect_tunnel');
}

export function listenConnectionState(callback: (state: string) => void) {
  return listen('connection-state', (event) => {
    callback(event.payload as string);
  });
}

// Auto-sync connection state to store
export function syncConnectionState() {
  const unlisten = listenConnectionState((state) => {
    connectionState.set(state as any);
  });
  return unlisten;
}