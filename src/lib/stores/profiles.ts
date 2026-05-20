import { writable } from 'svelte/store';

export interface Profile {
  id: string;
  label: string;
  host: string;
  port: number;
  username: string;
  auth_type: 'password' | 'key_inline' | 'key_file' | 'agent';
  identity_file_path?: string;
  created_at: string;
  updated_at: string;
}

export const profiles = writable<Profile[]>([]);