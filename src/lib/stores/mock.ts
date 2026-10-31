// Sample data for previewing the UI in a plain browser (no Tauri backend).
import type { Clip, Settings, Status } from '#lib/api.ts';

export function mockSettings(): Settings {
	return {
		version: 1,
		first_run_complete: true,
		pro_mode: false,
		theme: 'system',
		clips_dir: 'C:\\Users\\you\\AppData\\Roaming\\Rewind\\Clips',
		record_mode: 'when_game_detected',
		launch_on_startup: false,
		start_minimized: false,
		play_sound_on_save: true,
		video: {
			monitor: 0,
			height: 1080,
			fps: 60,
			codec: 'h264',
			encoder: 'auto',
			rate_control: 'vbr',
			bitrate_kbps: 20000,
			quality: 70,
			keyframe_secs: 1,
			capture_cursor: true,
			hdr: 'tonemap_to_sdr'
		},
		buffer: { seconds: 60, long_seconds: 0, post_roll_secs: 0, max_memory_mb: 1024, extend_secs: 15 },
		audio: [
			{ name: 'Desktop', enabled: true, source: { kind: 'system' }, volume: 1 },
			{ name: 'Microphone', enabled: false, source: { kind: 'mic', device_id: null }, volume: 1 },
			{ name: 'Discord', enabled: false, source: { kind: 'app', exe: 'Discord.exe' }, volume: 1 }
		],
		hotkeys: {
			save_clip: 'Alt+F8',
			save_long_clip: '',
			marker: 'Alt+F7',
			screenshot: 'Alt+F12',
			toggle_pause: 'Alt+F11',
			extend_last: ''
		},
		profiles: [],
		privacy_apps: ['KeePassXC.exe', '1Password.exe', 'Bitwarden.exe'],
		ssd_warn_gb_per_day: 50
	};
}

export function mockStatus(): Status {
	return {
		state: 'recording',
		buffered_secs: 47.3,
		buffer_secs: 60,
		fps: 59.9,
		bitrate_kbps: 18420,
		dropped: 0,
		memory_mb: 104.6,
		encoder: 'NVIDIA H.264 Encoder MFT',
		resolution: [2580, 1080],
		game: 'Counter-Strike 2',
		privacy: false,
		markers: [12.4, 31],
		error: null,
		manual_on: false,
		written_today_gb: 0.31
	};
}

export function mockClips(): Clip[] {
	const now = Date.now();
	const names = [
		'Counter-Strike 2 2026-10-31 21-14-02',
		'Counter-Strike 2 2026-10-31 20-58-41',
		'Rocket League 2026-10-30 23-02-10',
		'Desktop 2026-10-30 18-44-55',
		'Elden Ring 2026-10-29 22-31-07',
		'Elden Ring 2026-10-29 22-12-50'
	];
	return names.map((name, i) => ({
		path: `C:\\clips\\${name}.mp4`,
		name,
		size: 40e6 + i * 13e6,
		modified: now - i * 9e6,
		duration: [60, 60, 35, 12, 60, 90][i],
		thumb: null
	}));
}
