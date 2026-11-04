import { isTauri } from '@tauri-apps/api/core';
import { httpBackend } from './http';
import { installReplay } from './replay';
import { tauriBackend } from './tauri';

export * from './types';

// The live demo answers from a recording; builds without VITE_KUBRI_DEMO leave this out.
if (import.meta.env.VITE_KUBRI_DEMO && typeof window !== 'undefined') installReplay(import.meta.env.VITE_KUBRI_DEMO);

export const backend = isTauri() ? tauriBackend : httpBackend;
