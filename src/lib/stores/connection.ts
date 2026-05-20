import { writable } from 'svelte/store';

export type ConnectionState = 
  | 'disconnected' 
  | 'connecting' 
  | 'authenticating' 
  | 'tunnel-active' 
  | 'error';

export const connectionState = writable<ConnectionState>('disconnected');
export const connectionError = writable<string>('');