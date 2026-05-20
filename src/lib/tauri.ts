import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { connectionState, connectionError } from './stores/connection';
import { appendLog } from './stores/logs';
import type { Profile } from './stores/profiles';
import type { LogEntry } from './stores/logs';

// Type-safe wrapper for the greet command
export async function greet(name: string): Promise<string> {
  return await invoke('greet', { name });
}

// Generic typed event listener helper
export function onEvent<T>(eventName: string, callback: (payload: T) => void) {
  return listen<T>(eventName, (event) => callback(event.payload));
}

export async function connectTunnel(profileId: string): Promise<string> {
  return await invoke('connect_tunnel', { profileId });
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

export async function getProfileById(id: string): Promise<Profile> {
  return await invoke('get_profile_by_id_cmd', { id });
}

export async function updateProfile(req: {
  id: string;
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
  return await invoke('update_profile_cmd', { req });
}

// Connection state
export async function getConnectionState(): Promise<string> {
  return await invoke('get_connection_state_cmd');
}

// Log commands
export async function getLogs(level?: string, limit?: number): Promise<LogEntry[]> {
  return await invoke('get_logs_cmd', { level, limit });
}

export async function clearLogs(): Promise<void> {
  return await invoke('clear_logs_cmd');
}

export function listenLogs(callback: (entry: LogEntry) => void) {
  return listen<LogEntry>('log-entry', (event) => {
    callback(event.payload);
  });
}

// Auto-sync logs to store
export function syncLogs() {
  const unlisten = listenLogs((entry) => {
    appendLog(entry);
  });
  return unlisten;
}

// SSH config import
export interface SshConfigEntry {
  host_aliases: string[];
  hostname: string;
  user: string | null;
  port: number | null;
  identity_file: string | null;
}

export interface ParseResult {
  entries: SshConfigEntry[];
  skipped: string[];
}

export async function parseSshConfig(): Promise<ParseResult> {
  return await invoke('parse_ssh_config_cmd');
}

export async function importSshConfig(selectedHosts: string[]): Promise<Profile[]> {
  return await invoke('import_ssh_config_cmd', { selectedHosts });
}