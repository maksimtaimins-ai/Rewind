import { isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { api, type Clip, type Settings, type Status } from '#lib/api.ts';
import { mockClips, mockSettings, mockStatus } from './mock.ts';

export type ToastLevel = 'info' | 'success' | 'warning' | 'error';
export interface Toast {
	id: number;
	level: ToastLevel;
	message: string;
}

export const inApp = typeof window !== 'undefined' && isTauri();

export const app = $state({
	ready: false,
	settings: null as Settings | null,
	status: null as Status | null,
	clips: [] as Clip[],
	toasts: [] as Toast[],
	lastSaved: null as Clip | null
});

let toastId = 0;
export function toast(level: ToastLevel, message: string) {
	const id = ++toastId;
	app.toasts = [...app.toasts.slice(-3), { id, level, message }];
	setTimeout(() => (app.toasts = app.toasts.filter((t) => t.id !== id)), level === 'error' ? 8000 : 4000);
}

export async function refreshClips() {
	app.clips = inApp ? await api.clips() : mockClips();
}

let started = false;
export async function init() {
	if (started) return;
	started = true;
	if (!inApp) {
		// Browser preview (vite dev without Tauri): render with sample data.
		app.settings = mockSettings();
		app.status = mockStatus();
		app.clips = mockClips();
		app.ready = true;
		return;
	}
	const [settings, status] = await Promise.all([api.getSettings(), api.getStatus()]);
	app.settings = settings;
	app.status = status;
	await refreshClips();
	await listen<Status>('status', (e) => (app.status = e.payload));
	await listen<Clip>('clip-saved', (e) => {
		app.lastSaved = e.payload;
		app.clips = [e.payload, ...app.clips.filter((c) => c.path !== e.payload.path)];
	});
	await listen<{ level: ToastLevel; message: string }>('notice', (e) =>
		toast(e.payload.level, e.payload.message)
	);
	app.ready = true;
}

/** Apply a change to settings and persist it. */
export async function updateSettings(change: (s: Settings) => void) {
	if (!app.settings) return;
	const next = $state.snapshot(app.settings) as Settings;
	change(next);
	app.settings = next;
	if (!inApp) return;
	try {
		await api.setSettings(next);
	} catch (e) {
		toast('error', `Couldn't save settings: ${e}`);
	}
}
