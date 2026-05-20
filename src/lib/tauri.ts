import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { connectionState, connectionError } from './stores/connection';
import type { Profile } from './stores/profiles';

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

// Profile commands
export async function createProfile(profile: {
  label: string;
  host: string;
  port: number;
  username: string;
  auth_type: string;
  password?: string;
  private_key?: string;
  key_passphrase?: string;
  identity_file_path?: string;
}): Promise<Profile> {
  return await invoke('create_profile_cmd', { req: profile });
}

export async function getProfiles(): Promise<Profile[]> {
  return await invoke('get_profiles_cmd');
}

export async function deleteProfile(id: string): Promise<void> {
  return await invoke('delete_profile_cmd', { id });
}