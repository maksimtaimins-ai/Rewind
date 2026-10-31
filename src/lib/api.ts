import { invoke, convertFileSrc } from '@tauri-apps/api/core';

export type Codec = 'h264' | 'hevc';
export type EncoderChoice = 'auto' | 'nvenc' | 'amf' | 'qsv' | 'software';
export type RateControl = 'cbr' | 'vbr' | 'quality';
export type RecordMode = 'when_game_detected' | 'always' | 'manual';
export type HdrMode = 'tonemap_to_sdr' | 'passthrough';

export type AudioSource =
	| { kind: 'system' }
	| { kind: 'system_excluding'; exe: string }
	| { kind: 'app'; exe: string }
	| { kind: 'mic'; device_id: string | null };

export interface AudioTrack {
	name: string;
	enabled: boolean;
	source: AudioSource;
	volume: number;
}

export interface GameProfile {
	exe: string;
	name: string;
	fps: number | null;
	height: number | null;
	bitrate_kbps: number | null;
	buffer_seconds: number | null;
}

export interface Hotkeys {
	save_clip: string;
	save_long_clip: string;
	marker: string;
	screenshot: string;
	toggle_pause: string;
	extend_last: string;
}

export interface Settings {
	version: number;
	first_run_complete: boolean;
	pro_mode: boolean;
	theme: 'system' | 'dark' | 'light';
	clips_dir: string;
	record_mode: RecordMode;
	launch_on_startup: boolean;
	start_minimized: boolean;
	play_sound_on_save: boolean;
	video: {
		monitor: number;
		height: number;
		fps: number;
		codec: Codec;
		encoder: EncoderChoice;
		rate_control: RateControl;
		bitrate_kbps: number;
		quality: number;
		keyframe_secs: number;
		capture_cursor: boolean;
		hdr: HdrMode;
	};
	buffer: {
		seconds: number;
		long_seconds: number;
		post_roll_secs: number;
		max_memory_mb: number;
		extend_secs: number;
	};
	audio: AudioTrack[];
	hotkeys: Hotkeys;
	profiles: GameProfile[];
	privacy_apps: string[];
	ssd_warn_gb_per_day: number;
}

export type EngineState = 'idle' | 'recording' | 'paused' | 'error';

export interface Status {
	state: EngineState;
	buffered_secs: number;
	buffer_secs: number;
	fps: number;
	bitrate_kbps: number;
	dropped: number;
	memory_mb: number;
	encoder: string | null;
	resolution: [number, number] | null;
	game: string | null;
	privacy: boolean;
	/** Seconds ago each marker was set. */
	markers: number[];
	error: string | null;
	manual_on: boolean;
	written_today_gb: number;
}

export interface Clip {
	path: string;
	name: string;
	size: number;
	modified: number;
	duration: number;
	thumb: string | null;
}

export interface Monitor {
	index: number;
	name: string;
	width: number;
	height: number;
	primary: boolean;
}

export interface EncoderInfo {
	kind: 'nvenc' | 'amf' | 'qsv' | 'other' | 'software';
	name: string;
	codec: Codec;
}

export interface BenchResult extends EncoderInfo {
	fps: number;
	ok: boolean;
	error: string | null;
}

export interface Device {
	id: string;
	name: string;
}

export const api = {
	getSettings: () => invoke<Settings>('get_settings'),
	setSettings: (settings: Settings) => invoke<void>('set_settings', { settings }),
	defaultSettings: () => invoke<Settings>('default_settings'),
	getStatus: () => invoke<Status>('get_status'),
	monitors: () => invoke<Monitor[]>('list_monitors'),
	encoders: () => invoke<EncoderInfo[]>('list_encoders'),
	microphones: () => invoke<Device[]>('list_microphones'),
	benchmark: (secs: number) => invoke<BenchResult[]>('run_benchmark', { secs }),
	clips: () => invoke<Clip[]>('list_clips'),
	renameClip: (path: string, name: string) => invoke<Clip>('rename_clip', { path, name }),
	deleteClips: (paths: string[]) => invoke<void>('delete_clips', { paths }),
	clipsDir: () => invoke<string>('clips_dir'),
	saveNow: (secs?: number) => invoke<void>('save_clip_now', { secs: secs ?? null }),
	saveMarker: (index: number, before: number, after: number) =>
		invoke<void>('save_marker', { index, before, after }),
	setRecording: (on: boolean) => invoke<void>('set_recording', { on }),
	togglePause: () => invoke<boolean>('toggle_pause'),
	screenshot: () => invoke<void>('take_screenshot'),
	trim: (path: string, from: number, to: number) => invoke<Clip>('trim_clip', { path, from, to }),
	merge: (paths: string[]) => invoke<Clip>('merge_clips', { paths }),
	keyframes: (path: string) => invoke<[number[], number]>('clip_keyframes', { path }),
	exportSettings: (path: string) => invoke<void>('export_settings', { path }),
	importSettings: (path: string) => invoke<Settings>('import_settings', { path }),
	runningApps: () => invoke<string[]>('running_apps')
};

export const fileUrl = (path: string) => convertFileSrc(path);

export function fmtDuration(secs: number): string {
	const s = Math.max(0, Math.round(secs));
	const m = Math.floor(s / 60);
	return `${m}:${String(s % 60).padStart(2, '0')}`;
}

export function fmtBytes(n: number): string {
	if (n >= 1e9) return `${(n / 1e9).toFixed(1)} GB`;
	if (n >= 1e6) return `${(n / 1e6).toFixed(1)} MB`;
	return `${Math.max(1, Math.round(n / 1e3))} KB`;
}

export function fmtWhen(ms: number): string {
	const d = new Date(ms);
	const now = new Date();
	const time = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
	if (d.toDateString() === now.toDateString()) return `Today, ${time}`;
	const y = new Date(now);
	y.setDate(now.getDate() - 1);
	if (d.toDateString() === y.toDateString()) return `Yesterday, ${time}`;
	return `${d.toLocaleDateString([], { month: 'short', day: 'numeric' })}, ${time}`;
}
