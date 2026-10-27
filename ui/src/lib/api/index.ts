import { isTauri } from '@tauri-apps/api/core';
import { httpBackend } from './http';
import { tauriBackend } from './tauri';

export * from './types';

export const backend = isTauri() ? tauriBackend : httpBackend;
