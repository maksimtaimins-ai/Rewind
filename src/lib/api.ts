import { invoke, convertFileSrc } from '@tauri-apps/api/core';

export type Codec = 'h264' | 'hevc';
export type EncoderChoice = 'auto' | 'nvenc' | 'amf' | 'qsv' | 'software';
export type RateControl = 'cbr' | 'vbr' | 'quality';
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

/** Settings that switch in while a game runs; null keeps the global value. */
export interface GameProfile {
	exe: string;
	name: string;
	enabled: boolean;
	fps: number | null;
	height: number | null;
	bitrate_kbps: number | null;
	codec: Codec | null;
	/** Slot A. */
	buffer_seconds: number | null;
	slot_b_seconds: number | null;
	slot_c_seconds: number | null;
	hotkeys: Hotkeys | null;
	/** Save into `<clips folder>/<name>/`. */
	subfolder: boolean;
	keystroke_preset: string | null;
}

export interface Hotkeys {
	save_clip: string;
	marker: string;
	screenshot: string;
	extend_last: string;
	slow_mo: string;
	favorite_last: string;
	compact_hud: string;
}

export interface BufferSlot {
	enabled: boolean;
	seconds: number;
	hotkey: string;
}

export interface DiscordWebhook {
	name: string;
	url: string;
	/** Upload cap in MB: 10, or 25 for a boosted server. */
	limit_mb: 10 | 25;
}

export interface Settings {
	version: number;
	first_run_complete: boolean;
	/** The Live tab's four-step tour has been finished or skipped. */
	tour_complete: boolean;
	pro_mode: boolean;
	/** 'system' or a theme id from themes.ts. */
	theme: string;
	clips_dir: string;
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
		/** Slots B and C; slot A is `seconds` with `hotkeys.save_clip`. */
		extra_slots: BufferSlot[];
		max_memory_mb: number;
		extend_secs: number;
	};
	/** Opt-in: a normal save also records `seconds` (1-120) after the keypress. */
	post_roll: { enabled: boolean; seconds: number };
	slow_mo: { seconds: 2 | 3 | 5; speed: number };
	library: { session_gap_mins: number; purge_days: number };
	audio: AudioTrack[];
	hotkeys: Hotkeys;
	/** Off: profiles are never applied. */
	profiles_enabled: boolean;
	profiles: GameProfile[];
	privacy_apps: string[];
	ssd_warn_gb_per_day: number;
	sharing: {
		discord_webhooks: DiscordWebhook[];
		/** Listen for clips from other Rewinds and announce this PC. */
		lan_enabled: boolean;
		lan_port: number;
		/** Name other PCs see; empty = this PC's name. */
		device_name: string;
		device_id: string;
		/** PCs whose clips are accepted without asking. */
		trusted_peers: { id: string; name: string }[];
	};
	watch_folders: WatchFolder[];
	/** Compact HUD pill instead of the main window. Position in physical pixels. */
	hud: { enabled: boolean; x: number | null; y: number | null };
	/** Clips-drive free space limits, GB: warn below one, refuse saves below the other. */
	disk: { warn_gb: number; block_gb: number };
	smart_highlights: SmartHighlights;
	keystrokes: Keystrokes;
	/** Monitor index the on-screen overlays go on; null = the recorded monitor. */
	overlay_monitor: number | null;
}

export type KeyFilterMode = 'all' | 'allow' | 'block';
export type KeycapStyle = 'flat' | 'three_d' | 'minimal' | 'outlined' | 'mechanical';
export type OverlayFont = 'theme' | 'mono' | 'sans' | 'condensed';
export type PressAnimation = 'none' | 'pop' | 'flash' | 'spring' | 'pulse';
export type OverlayAnchor = 'top_left' | 'top_right' | 'bottom_left' | 'bottom_right' | 'top_center' | 'bottom_center' | 'custom';

/** Keystroke overlay. Off: no input hook is installed and nothing is recorded. */
export interface Keystrokes {
	enabled: boolean;
	/** Also draw input on screen as it happens. Shares style, position and filters. */
	live: boolean;
	keyboard: boolean;
	clicks: boolean;
	/** ~60 positions a second; large files. */
	movement: boolean;
	scroll: boolean;
	filter: KeyFilterMode;
	/** Key names for the allow or block list. */
	keys: string[];
	preset: string;
	pause_in_text_fields: boolean;
	style: {
		keycap: KeycapStyle;
		/** Keycap height in px at 1080p, 16-120. */
		size: number;
		font: OverlayFont;
		/** `#rrggbb`; empty = default. */
		base_color: string;
		/** `#rrggbb`; empty = automatic contrast. */
		text_color: string;
		border: boolean;
		border_color: string;
		radius: number;
		shadow: boolean;
		shadow_intensity: number;
		animation: PressAnimation;
		fade_secs: number;
		idle_opacity: number;
		pressed_opacity: number;
		hold_indicator: boolean;
		combo: 'chain' | 'stack';
	};
	position: {
		anchor: OverlayAnchor;
		/** Custom top-left, 0-1 of the frame. */
		x: number;
		y: number;
		margin: number;
		click_position: 'cursor' | 'with_keys';
		click_style: 'dot' | 'ring' | 'flash';
		trail: boolean;
		trail_ms: number;
	};
}

export interface MontageRequest {
	/** In playing order. */
	paths: string[];
	transition: 'cut' | 'crossfade';
	/** Longest piece per clip, seconds; null = whole clips. */
	max_secs: number | null;
	music: string | null;
	clip_audio: boolean;
}

export type StorageKind = 'clips' | 'screenshots' | 'exports' | 'recycled' | 'thumbnails' | 'app_data';

/** Disk use of Rewind's files by kind, measured on request. */
export interface StorageBreakdown {
	categories: { kind: StorageKind; bytes: number; files: number }[];
	total: number;
	drive_total: number | null;
	drive_free: number | null;
}

export interface KeystrokePreset {
	id: string;
	label: string;
	filter: KeyFilterMode;
	keys: string[];
	keyboard: boolean;
	clicks: boolean;
	scroll: boolean;
}

/** Audio-based smart highlights. Off: nothing is ever analysed. */
export interface SmartHighlights {
	enabled: boolean;
	/** dBFS, -30 to -3. */
	threshold_db: number;
	/** Also analyse each clip right after it's saved. */
	analyze_on_save: boolean;
}

/** A flagged moment in a clip, seconds from its start. */
export interface Moment {
	at: number;
	end: number;
	/** 0-1. */
	score: number;
	reason: 'spike' | 'voice' | 'sustained';
}

/** New .mp4 files in `input` are exported with `preset` into `output`. */
export interface WatchFolder {
	enabled: boolean;
	input: string;
	preset: ExportPreset;
	output: string;
	delete_original: boolean;
}

export interface WatchOp {
	at_ms: number;
	input: string;
	output: string | null;
	preset: ExportPreset;
	error: string | null;
	deleted: boolean;
}

export interface LanPeer {
	id: string;
	name: string;
	addr: string;
}

/** Progress of a LAN transfer, about once per 20 MB chunk. */
export interface LanProgress {
	direction: 'in' | 'out';
	peer: string;
	file: string;
	/** Sending only. */
	path?: string;
	done: number;
	total: number;
}

export interface LanOffer {
	offer: number;
	name: string;
	file: string;
	size: number;
}

export type HotkeyAction = 'save_clip' | 'marker' | 'screenshot' | 'extend_last' | 'slow_mo' | 'favorite_last' | 'toggle_hud';

export type EngineState = 'idle' | 'recording' | 'paused' | 'error';

export interface Status {
	state: EngineState;
	buffered_secs: number;
	buffer_secs: number;
	/** Every enabled replay length, slot A first. */
	slots: SlotStatus[];
	fps: number;
	bitrate_kbps: number;
	dropped: number;
	memory_mb: number;
	encoder: string | null;
	resolution: [number, number] | null;
	game: string | null;
	/** Game profile in effect. */
	profile: string | null;
	privacy: boolean;
	/** Markers set this session, oldest first. */
	markers: MarkerInfo[];
	/** A save still recording its post-roll. */
	post_roll: PostRollStatus | null;
	error: string | null;
	written_today_gb: number;
	/** Since Monday. */
	written_week_gb: number;
	/** Free space on the clips drive, null until measured. */
	disk_free_gb: number | null;
	/** `full`: new clip saves are refused (screenshots still work). */
	disk: 'ok' | 'low' | 'full';
	/** Captured audio loudness (RMS 0-1) in 100 ms steps since the last status, oldest first. */
	levels: number[];
}

export interface PostRollStatus {
	/** Seconds until the current save is written. */
	remaining: number;
	seconds: number;
	/** Further presses waiting behind it. */
	queued: number;
}

export interface SlotStatus {
	name: string;
	seconds: number;
	held: number;
	hotkey: string;
}

export interface MarkerInfo {
	id: number;
	/** Seconds since it was set. */
	ago: number;
	/** Unix milliseconds. */
	at_ms: number;
	/** The buffer still holds footage at the marker. */
	in_buffer: boolean;
}

export interface Clip {
	path: string;
	name: string;
	size: number;
	modified: number;
	duration: number;
	thumb: string | null;
	/** Changes when the thumbnail file is replaced. */
	thumb_rev: number;
	favorite: boolean;
	tags: string[];
	/** Smart-highlight moments; null until analysed. */
	highlights: Moment[] | null;
	/** Keystrokes were recorded with this clip. */
	has_input: boolean;
}

export type ExportPreset = 'discord10' | 'discord25' | 'shorts' | 'twitter' | 'gif';

export interface RecycledClip {
	id: string;
	name: string;
	original: string;
	deleted_ms: number;
	size: number;
	duration: number;
	thumb: string | null;
}

export interface LibraryMeta {
	/** Tag -> CSS colour. */
	tag_colors: Record<string, string>;
	/** Session key -> name. */
	sessions: Record<string, string>;
}

export interface Monitor {
	index: number;
	name: string;
	width: number;
	height: number;
	/** Top-left on the virtual desktop, physical pixels. */
	x: number;
	y: number;
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

/** Settings benchmark: the live pipeline measured over a few seconds. */
export interface BenchReport {
	secs: number;
	encoder: string;
	width: number;
	height: number;
	target_fps: number;
	capture_fps: number;
	encoded_fps: number;
	/** Highest rate the encoder could keep up, from how busy it was. */
	encoder_max_fps: number;
	/** 0-1. */
	encoder_load: number;
	dropped: number;
	bitrate_kbps: number;
	cpu_pct: number;
	rewind_cpu_pct: number;
	gpu_pct: number | null;
	gpu_encode_pct: number | null;
	ram_mb: number;
	buffer_mb: number;
	verdict: { grade: 'pass' | 'warn' | 'fail'; headline: string; detail: string[] };
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
	deleteClips: (paths: string[]) => invoke<number>('delete_clips', { paths }),
	recycled: () => invoke<RecycledClip[]>('list_recycled'),
	restoreClips: (ids: string[]) => invoke<number>('restore_clips', { ids }),
	setFavorite: (path: string, favorite: boolean) => invoke<Clip>('set_favorite', { path, favorite }),
	setTags: (path: string, tags: string[]) => invoke<Clip>('set_tags', { path, tags }),
	setTagColor: (tag: string, color: string) => invoke<void>('set_tag_color', { tag, color }),
	libraryMeta: () => invoke<LibraryMeta>('library_meta'),
	renameSession: (key: string, name: string) => invoke<void>('rename_session', { key, name }),
	clipsDir: () => invoke<string>('clips_dir'),
	saveNow: (secs?: number) => invoke<void>('save_clip_now', { secs: secs ?? null }),
	saveMarker: (id: number, secs: number) => invoke<void>('save_marker', { id, secs }),
	screenshot: () => invoke<void>('take_screenshot'),
	/** Do what a hotkey does; errors arrive as notices. */
	runAction: (action: HotkeyAction) => invoke<void>('run_action', { action }),
	trim: (path: string, from: number, to: number) => invoke<Clip>('trim_clip', { path, from, to }),
	merge: (paths: string[]) => invoke<Clip>('merge_clips', { paths }),
	/** `overlay`: draw recorded keystrokes in, when the clip has any. */
	exportClip: (path: string, preset: ExportPreset, overlay = false) => invoke<string>('export_clip', { path, preset, overlay }),
	keystrokePresets: () => invoke<KeystrokePreset[]>('keystroke_presets'),
	/** Resolves with the new clip. Progress (0-1): `montage-progress` events. */
	makeMontage: (request: MontageRequest) => invoke<Clip>('make_montage', { request }),
	storageBreakdown: () => invoke<StorageBreakdown>('storage_breakdown'),
	setThumbnail: (path: string, secs: number) => invoke<Clip>('set_thumbnail', { path, secs }),
	keyframes: (path: string) => invoke<[number[], number]>('clip_keyframes', { path }),
	exportSettings: (path: string) => invoke<void>('export_settings', { path }),
	importSettings: (path: string) => invoke<Settings>('import_settings', { path }),
	runningApps: () => invoke<string[]>('running_apps'),
	/** `webhook` is the index into `settings.sharing.discord_webhooks`. */
	postToDiscord: (path: string, webhook: number) => invoke<void>('post_to_discord', { path, webhook }),
	lanPeers: () => invoke<LanPeer[]>('lan_peers'),
	/** Resolves once the clip is saved on the other PC. */
	lanSend: (path: string, peer: string) => invoke<void>('lan_send', { path, peer }),
	lanAnswer: (offer: number, accept: boolean) => invoke<void>('lan_answer', { offer, accept }),
	watchHistory: () => invoke<WatchOp[]>('watch_history'),
	setCompactHud: (on: boolean) => invoke<void>('set_compact_hud', { on }),
	/** Right-click menu on the HUD pill. */
	hudMenu: () => invoke<void>('hud_menu'),
	liveBenchmark: (secs: number) => invoke<BenchReport>('run_live_benchmark', { secs }),
	/** Resolves with the number of moments found. Progress: `analysis-progress` events. */
	analyzeClips: (paths: string[]) => invoke<number>('analyze_clips', { paths })
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
