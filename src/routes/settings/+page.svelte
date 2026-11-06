<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { goto } from '$app/navigation';
	import { open, save } from '@tauri-apps/plugin-dialog';
	import { check } from '@tauri-apps/plugin-updater';
	import { api, type BenchReport, type BenchResult, type Device, type EncoderInfo, type Monitor, type Settings, type AudioSource } from '#lib/api.ts';
	import { reducedMotion } from '#lib/motion.ts';
	import { app, inApp, refreshStorage, toast, updateSettings } from '#lib/stores/app.svelte.ts';
	import StorageBar from '#lib/components/StorageBar.svelte';
	import Row from '#lib/components/Row.svelte';
	import Toggle from '#lib/components/Toggle.svelte';
	import Segmented from '#lib/components/Segmented.svelte';
	import HotkeyInput from '#lib/components/HotkeyInput.svelte';
	import ThemePicker from '#lib/components/ThemePicker.svelte';
	import WatchFolders from '#lib/components/WatchFolders.svelte';
	import BenchReportView from '#lib/components/BenchReport.svelte';
	import KeystrokeOverlay from '#lib/components/KeystrokeOverlay.svelte';
	import GameProfiles from '#lib/components/GameProfiles.svelte';
	import PerformanceList from '#lib/components/PerformanceList.svelte';

	const s = $derived(app.settings!);
	const pro = $derived(s.pro_mode);

	let monitors = $state<Monitor[]>([]);
	let encoders = $state<EncoderInfo[]>([]);
	let mics = $state<Device[]>([]);
	let bench = $state<BenchResult[] | null>(null);
	let benching = $state(false);
	let newPrivacy = $state('');
	let hookName = $state('');
	let hookUrl = $state('');
	// Same rule as the backend: https on a Discord host, a /webhooks/ path.
	const hookOk = $derived(/^https:\/\/(canary\.|ptb\.)?discord(app)?\.com\/.*webhooks\/\S+$/i.test(hookUrl.trim()));

	onMount(async () => {
		openForHash();
		refreshStorage();
		if (!inApp) {
			monitors = [{ index: 0, name: 'DISPLAY1', width: 2560, height: 1440, x: 0, y: 0, primary: true }];
			encoders = [
				{ kind: 'nvenc', name: 'NVIDIA H.264 Encoder MFT', codec: 'h264' },
				{ kind: 'nvenc', name: 'NVIDIA HEVC Encoder MFT', codec: 'hevc' }
			];
			return;
		}
		[monitors, encoders, mics] = await Promise.all([api.monitors(), api.encoders(), api.microphones()]);
	});

	const set = (f: (x: Settings) => void) => updateSettings(f);
	const num = (e: Event) => Number((e.currentTarget as HTMLInputElement).value);
	const monitorLabel = (m: Monitor) => `${m.name}, ${m.width}×${m.height} at ${m.x}, ${m.y}${m.primary ? ' (main)' : ''}`;

	const sections = $derived(
		[
			['essentials', 'Essentials'],
			['hotkeys', 'Hotkeys'],
			['appearance', 'Appearance'],
			['capture', 'Capture'],
			['performance', 'Performance'],
			['library', 'Library'],
			['storage', 'Storage'],
			['sharing', 'Sharing'],
			['games', 'Game profiles'],
			['help', 'Help'],
			['advanced', 'Advanced']
		].filter(([id]) => pro || id !== 'advanced')
	);

	// Advanced is folded away until asked for. A link or search result that
	// points inside it opens it first.
	let advancedOpen = $state(false);
	async function openForHash() {
		const id = decodeURIComponent(location.hash.slice(1));
		if (!id) return;
		let el = document.getElementById(id);
		if (el?.closest('details') && !advancedOpen) {
			advancedOpen = true;
			await tick();
			el = document.getElementById(id);
		}
		if (!el) return;
		el.scrollIntoView({ block: 'center', behavior: reducedMotion() ? 'auto' : 'smooth' });
		el.classList.remove('flash');
		void el.offsetWidth;
		el.classList.add('flash');
		el.addEventListener('animationend', () => el.classList.remove('flash'), { once: true });
	}

	async function replayTour() {
		await updateSettings((x) => (x.tour_complete = false));
		goto('/');
	}

	// Only runs on click: one request to GitHub Releases, nothing in the background.
	let updating = $state<'idle' | 'checking' | 'installing'>('idle');
	async function checkForUpdate() {
		if (!inApp) return toast('info', 'Rewind is up to date');
		updating = 'checking';
		try {
			const update = await check();
			if (!update) return toast('info', 'Rewind is up to date');
			updating = 'installing';
			toast('info', `Installing Rewind ${update.version}. It restarts when done.`);
			// The installer closes Rewind itself, then relaunches it.
			await update.downloadAndInstall();
		} catch (e) {
			toast('error', `Update check failed: ${e}`);
		} finally {
			updating = 'idle';
		}
	}

	const kinds = $derived([...new Set(encoders.filter((e) => e.codec === s.video.codec).map((e) => e.kind))]);
	const kindLabel: Record<string, string> = {
		auto: 'Best available',
		nvenc: 'NVIDIA NVENC',
		amf: 'AMD AMF',
		qsv: 'Intel Quick Sync',
		software: 'CPU (slow)'
	};

	// Rough RAM use of the encoded buffer, to make the cost of a long buffer visible.
	// Slots share one buffer, so its size follows the longest enabled one.
	const longest = $derived(Math.max(s.buffer.seconds, ...s.buffer.extra_slots.filter((x) => x.enabled).map((x) => x.seconds)));
	const ramEstimate = $derived(Math.round(((s.video.bitrate_kbps * 1000) / 8 / 1e6) * longest));

	function sourceLabel(src: AudioSource) {
		switch (src.kind) {
			case 'system':
				return 'Everything you hear';
			case 'system_excluding':
				return `Everything except ${src.exe}`;
			case 'app':
				return `Only ${src.exe}`;
			case 'mic':
				return mics.find((m) => m.id === src.device_id)?.name ?? 'Default microphone';
		}
	}

	async function pickFolder() {
		if (!inApp) return;
		const dir = await open({ directory: true, defaultPath: s.clips_dir, title: 'Save clips to' });
		if (typeof dir === 'string') set((x) => (x.clips_dir = dir));
	}

	async function runBench() {
		benching = true;
		bench = null;
		try {
			bench = await api.benchmark(5);
		} catch (e) {
			toast('error', String(e));
		} finally {
			benching = false;
		}
	}

	let live = $state<BenchReport | null>(null);
	let liveRunning = $state(false);
	async function runLiveBench() {
		liveRunning = true;
		live = null;
		try {
			live = inApp
				? await api.liveBenchmark(10)
				: {
						secs: 10, encoder: 'NVIDIA H.264 Encoder MFT', width: 2560, height: 1440, target_fps: 60, capture_fps: 59.8, encoded_fps: 59.8,
						encoder_max_fps: 214, encoder_load: 0.28, dropped: 0, bitrate_kbps: 19800, cpu_pct: 34, rewind_cpu_pct: 1.6,
						gpu_pct: 71, gpu_encode_pct: 27, ram_mb: 412, buffer_mb: 318,
						verdict: { grade: 'pass', headline: 'Your current settings are fine for 1440p 60fps gameplay.', detail: [] }
					};
		} catch (e) {
			toast('error', `Benchmark failed: ${e}`);
		} finally {
			liveRunning = false;
		}
	}

	async function exportProfile() {
		const path = await save({ defaultPath: 'rewind-settings.toml', filters: [{ name: 'Rewind settings', extensions: ['toml'] }] });
		if (path) {
			await api.exportSettings(path);
			toast('success', 'Settings exported');
		}
	}

	async function importProfile() {
		const path = await open({ filters: [{ name: 'Rewind settings', extensions: ['toml'] }] });
		if (typeof path === 'string') {
			try {
				app.settings = await api.importSettings(path);
				toast('success', 'Settings imported');
			} catch (e) {
				toast('error', `Couldn't import: ${e}`);
			}
		}
	}

</script>

<svelte:window onhashchange={openForHash} />

<div class="page">
	<aside class="index">
		<h1>Settings</h1>
		<ul>
			{#each sections as [id, label] (id)}
				<li><a href="#{id}">{label}</a></li>
			{/each}
		</ul>
	</aside>

	<div class="body">
		<div class="pro">
			<Row label="Pro mode" hint="Show encoder, bitrate, codec and other advanced settings.">
				<Toggle checked={pro} label="Pro mode" onchange={(v) => set((x) => (x.pro_mode = v))} />
			</Row>
		</div>

		<section id="essentials">
			<h2>Essentials</h2>
			<Row label="Buffer length" hint="Keeps the last {s.buffer.seconds >= 60 && s.buffer.seconds % 60 === 0 ? `${s.buffer.seconds / 60} ${s.buffer.seconds === 60 ? 'minute' : 'minutes'}` : `${s.buffer.seconds} seconds`} of your screen in memory, about {ramEstimate} MB of RAM.">
				<Segmented
					label="Buffer length"
					value={s.buffer.seconds}
					options={[
						{ value: 10, label: '10s' },
						{ value: 30, label: '30s' },
						{ value: 60, label: '1m' },
						{ value: 120, label: '2m' },
						{ value: 300, label: '5m' }
					]}
					onchange={(v) => set((x) => (x.buffer.seconds = v))}
				/>
				<input class="numbox num" type="number" min="10" max="300" value={s.buffer.seconds} onchange={(e) => set((x) => (x.buffer.seconds = Math.min(300, Math.max(10, num(e)))))} aria-label="Buffer length in seconds" />
				<span class="unit">s</span>
			</Row>
			<Row label="Save hotkey" hint="Saves the last {s.buffer.seconds} seconds (slot A).">
				<HotkeyInput label="Save hotkey" value={s.hotkeys.save_clip} onchange={(v) => set((x) => (x.hotkeys.save_clip = v))} />
			</Row>
			<Row label="Save folder" hint={s.clips_dir}>
				<button class="secondary" onclick={pickFolder}>Change</button>
			</Row>
			<Row label="Encoder" hint="What compresses the video. Hardware keeps the CPU free for the game." help="Your graphics card (NVIDIA, AMD or Intel) has a chip just for compressing video, so recording costs almost no frames. Software uses the processor instead: it works anywhere, but can slow the game down. Auto picks the graphics card when there is one.">
				<select value={s.video.encoder} onchange={(e) => set((x) => (x.video.encoder = (e.currentTarget as HTMLSelectElement).value as Settings['video']['encoder']))} aria-label="Encoder">
					<option value="auto">{kindLabel.auto}</option>
					{#each kinds as k (k)}
						<option value={k}>{kindLabel[k] ?? k}</option>
					{/each}
				</select>
			</Row>
		</section>

		<section id="hotkeys">
			<h2>Hotkeys</h2>
			<Row label="Save clip" hint="Saves the last {s.buffer.seconds} seconds (slot A).">
				<HotkeyInput label="Save clip" value={s.hotkeys.save_clip} onchange={(v) => set((x) => (x.hotkeys.save_clip = v))} />
			</Row>
			<Row label="Mark this moment" hint="Pick how much to keep later, from the Live tab.">
				<HotkeyInput label="Marker" value={s.hotkeys.marker} onchange={(v) => set((x) => (x.hotkeys.marker = v))} />
			</Row>
			<Row label="Screenshot" hint="Saves the current frame as a PNG.">
				<HotkeyInput label="Screenshot" value={s.hotkeys.screenshot} onchange={(v) => set((x) => (x.hotkeys.screenshot = v))} />
			</Row>
			<Row label="Favorite last clip" hint="Stars the clip you saved most recently.">
				<HotkeyInput label="Favorite last clip" value={s.hotkeys.favorite_last} onchange={(v) => set((x) => (x.hotkeys.favorite_last = v))} />
			</Row>
			<Row label="Extend last clip" hint="Adds {s.buffer.extend_secs}s before your last clip and replaces it.">
				<HotkeyInput label="Extend last clip" value={s.hotkeys.extend_last} onchange={(v) => set((x) => (x.hotkeys.extend_last = v))} />
			</Row>
			<Row label="Slow-mo replay" hint="Last {s.slow_mo.seconds}s at {s.slow_mo.speed}×, {Math.round(s.slow_mo.seconds / s.slow_mo.speed)}s long. Video only.">
				<HotkeyInput label="Slow-mo replay" value={s.hotkeys.slow_mo} onchange={(v) => set((x) => (x.hotkeys.slow_mo = v))} />
			</Row>
			<Row label="Compact HUD" hint="Switches between the floating pill and this window.">
				<HotkeyInput label="Compact HUD" value={s.hotkeys.compact_hud} onchange={(v) => set((x) => (x.hotkeys.compact_hud = v))} />
			</Row>
			<Row label="Slow-mo length" hint="How much of the buffer to slow down.">
				<Segmented
					label="Slow-mo length"
					value={s.slow_mo.seconds}
					options={[2, 3, 5].map((v) => ({ value: v as 2 | 3 | 5, label: `${v}s` }))}
					onchange={(v) => set((x) => (x.slow_mo.seconds = v))}
				/>
			</Row>
			<Row label="Slow-mo speed" hint="Lower is slower.">
				<input type="range" min="0.1" max="0.5" step="0.05" value={s.slow_mo.speed} onchange={(e) => set((x) => (x.slow_mo.speed = num(e)))} aria-label="Slow-mo speed" />
				<span class="readout num">{s.slow_mo.speed.toFixed(2)}×</span>
			</Row>
		</section>

		<section id="appearance">
			<h2>Appearance</h2>
			<div class="block">
				<div class="block-label">Theme</div>
				<div class="block-hint">Follow Windows, or pick one.</div>
				<ThemePicker value={s.theme} onchange={(v) => set((x) => (x.theme = v))} />
			</div>
			<Row id="s-compact-hud-toggle" label="Compact HUD" hint="Swap this window for a small always-on-top pill with the buffer and a Save button.{s.hotkeys.compact_hud ? ` ${s.hotkeys.compact_hud} switches back and forth.` : ''}">
				<Toggle checked={s.hud.enabled} label="Compact HUD" onchange={(v) => set((x) => (x.hud.enabled = v))} />
			</Row>
			<Row label="Open minimized to the tray" hint="The window stays hidden until you open it.">
				<Toggle checked={s.start_minimized} label="Open minimized" onchange={(v) => set((x) => (x.start_minimized = v))} />
			</Row>
			<Row label="Play a sound on save" hint="A short click when a clip is written.">
				<Toggle checked={s.play_sound_on_save} label="Sound on save" onchange={(v) => set((x) => (x.play_sound_on_save = v))} />
			</Row>
		</section>

		<section id="capture">
			<h2>Capture</h2>
			<Row label="Record extra after press" hint={s.post_roll.enabled ? `Keeps recording ${s.post_roll.seconds}s after you save. Markers, slow-mo and screenshots are unaffected.` : 'Off. A clip ends the moment you press save.'}>
				<Toggle checked={s.post_roll.enabled} label="Post-roll" onchange={(v) => set((x) => (x.post_roll.enabled = v))} />
			</Row>
			{#if s.post_roll.enabled}
				<Row label="Post-roll length" hint="1 to 120 seconds, for every save slot.">
					<input type="range" min="1" max="120" step="1" value={s.post_roll.seconds} onchange={(e) => set((x) => (x.post_roll.seconds = num(e)))} aria-label="Post-roll length" />
					<span class="readout num">{s.post_roll.seconds}s</span>
				</Row>
			{/if}
			{#each s.buffer.extra_slots as slot, i (i)}
				{#if i === 0 || pro}
					<Row label="Slot {'BC'[i]}" hint={slot.enabled ? `A second replay length${slot.hotkey ? ` on ${slot.hotkey}` : ''}. Shares the same buffer.` : 'Off. A second replay length with its own hotkey.'}>
						{#if slot.enabled}
							<input class="numbox num" type="number" min="10" max="300" value={slot.seconds} onchange={(e) => set((x) => (x.buffer.extra_slots[i].seconds = Math.min(300, Math.max(10, num(e)))))} aria-label="Slot {'BC'[i]} length in seconds" />
							<span class="unit">s</span>
							<HotkeyInput label="Slot {'BC'[i]} hotkey" value={slot.hotkey} onchange={(v) => set((x) => (x.buffer.extra_slots[i].hotkey = v))} />
						{/if}
						<Toggle checked={slot.enabled} label="Use slot {'BC'[i]}" onchange={(v) => set((x) => (x.buffer.extra_slots[i].enabled = v))} />
					</Row>
				{/if}
			{/each}
			{#if ramEstimate > 2048}
				<p class="warn">The buffer needs about {(ramEstimate / 1024).toFixed(1)} GB of RAM for a {longest}-second slot. Shorten the longest slot or lower the bitrate.</p>
			{/if}
			<Row label="Extend last clip by" hint="Footage added before your last clip's start, if the buffer has it.">
				<input class="numbox num" type="number" min="5" max="120" value={s.buffer.extend_secs} onchange={(e) => set((x) => (x.buffer.extend_secs = Math.min(120, Math.max(5, num(e)))))} aria-label="Extend seconds" />
				<span class="unit">s</span>
			</Row>
			{#if monitors.length > 1}
				<Row label="Screen" hint="The monitor Rewind records.">
					<select value={s.video.monitor} onchange={(e) => set((x) => (x.video.monitor = num(e)))} aria-label="Screen">
						{#each monitors as m (m.index)}
							<option value={m.index}>{monitorLabel(m)}</option>
						{/each}
					</select>
				</Row>
			{/if}
			<Row label="Save resolution" hint="Your screen is captured at full size; clips are scaled to this on the graphics card." help="Recording below your screen's resolution makes smaller files and gives the encoder less to do. Rewind always captures at your screen's own size and shrinks the picture on the graphics card while encoding, so a 1440p screen can save 720p or 1080p clips with no extra cost to the game. Native keeps every detail.">
				<Segmented
					label="Save resolution"
					value={s.video.height}
					options={[
						{ value: 720, label: '720p' },
						{ value: 1080, label: '1080p' },
						{ value: 1440, label: '1440p' },
						{ value: 0, label: 'Native' }
					]}
					onchange={(v) => set((x) => (x.video.height = v))}
				/>
			</Row>
			<Row label="Frame rate" hint={s.performance.fps_mode === 'display' ? 'Matches the refresh rate of your screen. A game running slower records at its own pace.' : 'A cap. Variable refresh is followed as it happens.'}>
				<Segmented
					label="Frame rate mode"
					value={s.performance.fps_mode}
					options={[
						{ value: 'fixed', label: 'Fixed' },
						{ value: 'display', label: 'Match display' }
					]}
					onchange={(v) => set((x) => (x.performance.fps_mode = v))}
				/>
				{#if s.performance.fps_mode === 'fixed'}
					<Segmented
						label="Frame rate"
						value={s.video.fps}
						options={[30, 60, 120, 144].map((f) => ({ value: f, label: String(f) }))}
						onchange={(v) => set((x) => (x.video.fps = v))}
					/>
				{/if}
			</Row>
			<Row label="Record the mouse cursor" hint="Draws the pointer into clips.">
				<Toggle checked={s.video.capture_cursor} label="Record cursor" onchange={(v) => set((x) => (x.video.capture_cursor = v))} />
			</Row>
			<Row label="Launch when Windows starts" hint="Starts in the tray with the buffer running.">
				<Toggle checked={s.launch_on_startup} label="Launch when Windows starts" onchange={(v) => set((x) => (x.launch_on_startup = v))} />
			</Row>
			{#if pro}
			<Row label="Bitrate" hint="Higher keeps fast motion sharp; files get bigger." help="How much data each second of video gets. Fast scenes like grass, smoke and quick turns need more to stay sharp. Around 20 Mb/s at 1080p and 60 fps looks good for most games; double it for 1440p or very busy games.">
				<input type="range" min="2000" max="80000" step="1000" value={s.video.bitrate_kbps} onchange={(e) => set((x) => (x.video.bitrate_kbps = num(e)))} aria-label="Bitrate" />
				<span class="readout num">{(s.video.bitrate_kbps / 1000).toFixed(0)} Mb/s</span>
			</Row>
			{/if}


			<h3>Audio</h3>
			<p class="intro">Each source is its own track, so editors can mix them. Players play the first.</p>
			{#each s.audio as t, i (i)}
				<Row label={t.name} hint={sourceLabel(t.source)}>
					{#if pro && t.source.kind === 'mic'}
						<select
							value={t.source.device_id ?? ''}
							onchange={(e) => set((x) => (x.audio[i].source = { kind: 'mic', device_id: (e.currentTarget as HTMLSelectElement).value || null }))}
							aria-label="Microphone for {t.name}"
						>
							<option value="">Default microphone</option>
							{#each mics as m (m.id)}
								<option value={m.id}>{m.name}</option>
							{/each}
						</select>
					{/if}
					{#if pro}
						<input
							type="range"
							min="0"
							max="2"
							step="0.05"
							value={t.volume}
							onchange={(e) => set((x) => (x.audio[i].volume = num(e)))}
							aria-label="{t.name} volume"
						/>
						<span class="readout num">{Math.round(t.volume * 100)}%</span>
					{/if}
					<Toggle checked={t.enabled} label="Record {t.name}" onchange={(v) => set((x) => (x.audio[i].enabled = v))} />
				</Row>
			{/each}
			{#if pro}
				<div class="add">
					<button class="secondary" onclick={() => set((x) => x.audio.push({ name: 'Music', enabled: true, source: { kind: 'app', exe: 'Spotify.exe' }, volume: 1 }))}>
						Add app track
					</button>
					<button class="secondary" onclick={() => set((x) => x.audio.push({ name: 'Game only', enabled: true, source: { kind: 'system_excluding', exe: 'Discord.exe' }, volume: 1 }))}>
						Add game-only track
					</button>
				</div>
			{/if}

			<h3>Privacy</h3>
			<p class="intro">No account, no cloud, no telemetry. Capture pauses while one of these apps is focused.</p>
			<ul class="chips">
				{#each s.privacy_apps as a, i (a)}
					<li>
						{a}
						<button aria-label="Stop pausing for {a}" onclick={() => set((x) => x.privacy_apps.splice(i, 1))}>×</button>
					</li>
				{/each}
			</ul>
			<form
				class="add"
				onsubmit={(e) => {
					e.preventDefault();
					const v = newPrivacy.trim();
					if (v && !s.privacy_apps.includes(v)) set((x) => x.privacy_apps.push(v.endsWith('.exe') ? v : `${v}.exe`));
					newPrivacy = '';
				}}
			>
				<input type="text" placeholder="App name, e.g. chrome.exe" bind:value={newPrivacy} aria-label="App to pause for" />
				<button class="secondary" type="submit">Add</button>
			</form>

			<KeystrokeOverlay />
		</section>

		<section id="performance">
			<h2>Performance</h2>
			<p class="intro">For PCs that struggle while gaming. These sit on top of your settings; turn them off and everything is as you set it.</p>
			<Row id="s-low-spec" label="Low-spec mode" hint="720p at 30 fps, HEVC at about 4 Mb/s, a 30 second buffer, no overlays." help="One switch for weaker PCs: smaller picture, half the frames, a quarter of the data and no extras running. Clips are lighter on the graphics card and on RAM. HEVC is used when your graphics card can encode it, otherwise H.264.">
				<Toggle checked={s.performance.low_spec} label="Low-spec mode" onchange={(v) => set((x) => (x.performance.low_spec = v))} />
			</Row>
			<Row id="s-battery-throttle" label="Save power on battery" hint="Switches to the low-spec preset while a laptop is unplugged, and back when it is plugged in.">
				<Toggle checked={s.performance.battery_throttle} label="Save power on battery" onchange={(v) => set((x) => (x.performance.battery_throttle = v))} />
			</Row>
		</section>

		<section id="library">
			<h2>Library</h2>
			<Row label="Group into sessions" hint="Clips saved this close together show as one session.">
				<input class="numbox num" type="number" min="1" max="1440" value={s.library.session_gap_mins} onchange={(e) => set((x) => (x.library.session_gap_mins = Math.min(1440, Math.max(1, num(e)))))} aria-label="Session gap in minutes" />
				<span class="unit">min</span>
			</Row>
			<Row label="Open clips fullscreen" hint="Playing a clip fills the screen. Press F or Esc to switch back and forth.">
					<Toggle checked={s.playback.fullscreen_on_open} label="Open clips fullscreen" onchange={(v) => set((x) => (x.playback.fullscreen_on_open = v))} />
				</Row>
				<Row label="Keep deleted clips for" hint="Then they're removed from Recently deleted for good.">
				<input class="numbox num" type="number" min="1" max="365" value={s.library.purge_days} onchange={(e) => set((x) => (x.library.purge_days = Math.min(365, Math.max(1, num(e)))))} aria-label="Days to keep deleted clips" />
				<span class="unit">days</span>
			</Row>
		</section>

		<section id="storage">
			<h2>Storage</h2>
			<div class="storage-bar">
				<StorageBar data={app.storage} />
			</div>
			<Row label="Free on the clips drive" hint={app.status?.disk === 'full' ? `Under ${s.disk.block_gb} GB: saving clips is paused.` : app.status?.disk === 'low' ? `Under ${s.disk.warn_gb} GB: running low.` : 'Checked every few seconds.'}>
				<span class="readout num wide">{app.status?.disk_free_gb != null ? `${app.status.disk_free_gb.toFixed(1)} GB` : '—'}</span>
			</Row>
			<Row label="Written today" hint="Clips, trims and merges Rewind wrote to disk.">
				<span class="readout num wide">{(app.status?.written_today_gb ?? 0).toFixed(2)} GB</span>
			</Row>
			<Row label="Written this week" hint="Since Monday.">
				<span class="readout num wide">{(app.status?.written_week_gb ?? 0).toFixed(2)} GB</span>
			</Row>
		</section>

		<section id="sharing">
			<h2>Sharing</h2>
			<p class="intro">Discord webhooks post a clip straight into a channel. Make one in the channel's settings under Integrations, then paste its URL here. Anyone with the URL can post, so keep it private.</p>
			{#each s.sharing.discord_webhooks as w, i (i)}
				<Row label={w.name || 'Unnamed webhook'} hint={w.url.replace(/\/[^/]+$/, '/••••')}>
					<Segmented
						label="Upload limit for {w.name}"
						value={w.limit_mb}
						options={[
							{ value: 10, label: '10 MB' },
							{ value: 25, label: '25 MB, boosted' }
						]}
						onchange={(v) => set((x) => (x.sharing.discord_webhooks[i].limit_mb = v))}
					/>
					<button class="link" onclick={() => set((x) => x.sharing.discord_webhooks.splice(i, 1))}>Remove</button>
				</Row>
			{/each}
			<form
				class="add"
				onsubmit={(e) => {
					e.preventDefault();
					if (!hookOk) return;
					const name = hookName.trim() || 'Discord';
					const url = hookUrl.trim();
					set((x) => x.sharing.discord_webhooks.push({ name, url, limit_mb: 10 }));
					hookName = '';
					hookUrl = '';
				}}
			>
				<input type="text" placeholder="Name, e.g. Squad #clips" bind:value={hookName} aria-label="Webhook name" />
				<input class="grow" type="text" placeholder="https://discord.com/api/webhooks/…" bind:value={hookUrl} aria-label="Webhook URL" />
				<button class="secondary" type="submit" disabled={!hookOk}>Add webhook</button>
			</form>

			<h3>LAN send</h3>
			<Row label="Send clips between PCs" hint={s.sharing.lan_enabled ? 'Other Rewinds on this network can see this PC and send it clips. Nothing leaves your network.' : 'Off. Turn on to send clips to other PCs running Rewind on this network.'}>
				<Toggle checked={s.sharing.lan_enabled} label="LAN send" onchange={(v) => set((x) => (x.sharing.lan_enabled = v))} />
			</Row>
			{#if s.sharing.lan_enabled}
				<Row label="This PC's name" hint="What other PCs see in their Send to list.">
					<input type="text" placeholder="This PC's name" value={s.sharing.device_name} onchange={(e) => set((x) => (x.sharing.device_name = (e.currentTarget as HTMLInputElement).value.trim()))} aria-label="Device name" />
				</Row>
				<Row label="Port" hint="Windows may ask to let Rewind through the firewall the first time.">
					<input class="numbox num" type="number" min="1024" max="65535" value={s.sharing.lan_port} onchange={(e) => set((x) => (x.sharing.lan_port = Math.min(65535, Math.max(1024, num(e)))))} aria-label="LAN port" />
				</Row>
				{#each s.sharing.trusted_peers as p, i (p.id)}
					<Row label={p.name} hint="Clips from this PC arrive without asking.">
						<button class="link" onclick={() => set((x) => x.sharing.trusted_peers.splice(i, 1))}>Forget</button>
					</Row>
				{/each}
			{/if}

			<h3 id="watch">Watch folders</h3>
			<WatchFolders />

			<h3 id="presence">Discord status</h3>
			<Row id="s-discord-presence" label="Show my game on Discord" hint="Shows Playing a game with Rewind on your Discord profile while a game is in front. Needs the Discord app running on this PC. Off: Rewind never talks to Discord.">
				<Toggle checked={s.sharing.discord_presence} label="Discord Rich Presence" onchange={(v) => set((x) => (x.sharing.discord_presence = v))} />
			</Row>
		</section>

		<section id="games">
			<h2>Game profiles</h2>
			<GameProfiles />
		</section>

		<section id="help">
			<h2>Help</h2>
			<Row label="Tour of the Live tab" hint="Four quick pointers: saving, the buffer, your clips and settings.">
				<button class="secondary" onclick={replayTour}>Show the tour</button>
			</Row>
			<Row label="Updates" hint="Checks GitHub Releases for a newer signed version. Rewind never checks on its own.">
				<button class="secondary" disabled={updating !== 'idle'} onclick={checkForUpdate}>
					{updating === 'checking' ? 'Checking…' : updating === 'installing' ? 'Installing…' : 'Check for updates'}
				</button>
			</Row>
		</section>

		{#if pro}
			<section id="advanced">
				<details bind:open={advancedOpen}>
					<summary><h2>Advanced</h2><span>Encoder tuning, monitors, benchmarks and diagnostics</span></summary>
					<h3 id="tuning">Encoder tuning</h3>
				<Row label="Codec" hint="HEVC is smaller; some editors and older devices can't play it." help="The video format. H.264 plays everywhere: Discord, phones, every editor. HEVC (H.265) looks the same at about half the size, but some browsers, older phones and editors can't open it. Pick H.264 if you share clips a lot.">
					<Segmented
						label="Codec"
						value={s.video.codec}
						options={[
							{ value: 'h264', label: 'H.264' },
							{ value: 'hevc', label: 'HEVC' }
						]}
						onchange={(v) => set((x) => (x.video.codec = v))}
					/>
				</Row>
				<Row label="Rate control" hint="How the bitrate is spent. Variable suits most games." help="Constant spends the same data every second, which keeps file sizes predictable. Variable spends more on busy scenes and less on calm ones, so it looks better at the same size. Quality ignores size and aims for a look you choose.">
					<Segmented
						label="Rate control"
						value={s.video.rate_control}
						options={[
							{ value: 'vbr', label: 'Variable' },
							{ value: 'cbr', label: 'Constant' },
							{ value: 'quality', label: 'Quality' }
						]}
						onchange={(v) => set((x) => (x.video.rate_control = v))}
					/>
				</Row>
				{#if s.video.rate_control === 'quality'}
					<Row label="Quality" hint="1 to 100. Higher looks better and makes bigger files." help="Used when rate control is set to Quality. The encoder keeps every scene at this look and lets the file size follow. 70 is close to what you see on screen; above 85 files grow fast for little visible gain.">
						<input class="numbox num" type="number" min="1" max="100" value={s.video.quality} onchange={(e) => set((x) => (x.video.quality = num(e)))} aria-label="Quality" />
					</Row>
				{/if}
				<Row label="Keyframe every" hint="Clips start on keyframes. Shorter is more precise, slightly larger." help="A keyframe is a complete picture; the frames between only store what changed since. A clip or trim has to start on a keyframe, so with one every second you can cut to within a second. Longer gaps save a little space.">
					<Segmented
						label="Keyframe interval"
						value={s.video.keyframe_secs}
						options={[0.5, 1, 2].map((v) => ({ value: v, label: `${v}s` }))}
						onchange={(v) => set((x) => (x.video.keyframe_secs = v))}
					/>
				</Row>
				<Row label="Memory limit" hint="Oldest seconds are dropped first above this." help="The buffer lives in RAM, never on disk, until you save. If high bitrates or a long buffer would grow it past this, Rewind drops the oldest seconds so your game keeps the memory it needs.">
					<input class="numbox num" type="number" min="128" max="16384" step="128" value={s.buffer.max_memory_mb} onchange={(e) => set((x) => (x.buffer.max_memory_mb = num(e)))} aria-label="Memory limit in MB" />
					<span class="unit">MB</span>
				</Row>
				<Row label="Warn after writing" hint="Daily total, to spare your SSD. {app.status?.written_today_gb.toFixed(2) ?? 0} GB so far today.">
					<input class="numbox num" type="number" min="0" max="2000" value={s.ssd_warn_gb_per_day} onchange={(e) => set((x) => (x.ssd_warn_gb_per_day = num(e)))} aria-label="Daily write warning in GB" />
					<span class="unit">GB</span>
				</Row>
				<Row label="Low disk warning" hint="Show a warning on the Live tab below this much free space.">
					<input class="numbox num" type="number" min="1" max="2000" value={s.disk.warn_gb} onchange={(e) => set((x) => (x.disk.warn_gb = Math.min(2000, Math.max(1, num(e)))))} aria-label="Warn below GB free" />
					<span class="unit">GB</span>
				</Row>
				<Row label="Stop saving below" hint="New clips are refused under this; screenshots still save. 0 turns it off.">
					<input class="numbox num" type="number" min="0" max={s.disk.warn_gb} value={s.disk.block_gb} onchange={(e) => set((x) => (x.disk.block_gb = Math.min(x.disk.warn_gb, Math.max(0, num(e)))))} aria-label="Stop saving below GB free" />
					<span class="unit">GB</span>
				</Row>

					<h3 id="monitors">Monitors</h3>
				{#if monitors.length > 1}
					<Row label="Recorded screen" hint="Pick the monitor Rewind records by hand." help="Rewind records one monitor, never a span across several. Positions are in physical pixels on your desktop; a screen left of or above the main one has negative numbers, which is normal.">
						<select value={s.video.monitor} onchange={(e) => set((x) => (x.video.monitor = num(e)))} aria-label="Recorded screen">
							{#each monitors as m (m.index)}
								<option value={m.index}>{monitorLabel(m)}</option>
							{/each}
						</select>
					</Row>
					<Row label="Overlay screen" hint="Where the live keystrokes and the HUD's default spot appear." help="Each overlay covers exactly one monitor. Same as recorded keeps them on the screen being captured, which is usually what you want.">
						<select
							value={s.overlay_monitor ?? -1}
							onchange={(e) => set((x) => (x.overlay_monitor = num(e) < 0 ? null : num(e)))}
							aria-label="Overlay screen"
						>
							<option value={-1}>Same as recorded</option>
							{#each monitors as m (m.index)}
								<option value={m.index}>{monitorLabel(m)}</option>
							{/each}
						</select>
					</Row>
				{/if}

					<h3 id="benchmarks">Benchmarks</h3>
				<Row label="Test current settings" hint="Watches 10 seconds of real capture at these settings and says whether they hold up. Best run while a game is moving.">
					<button class="secondary" disabled={liveRunning} onclick={runLiveBench}>{liveRunning ? 'Measuring… 10s' : 'Run benchmark'}</button>
				</Row>
				{#if live}
					<BenchReportView report={live} />
				{/if}
				<Row label="Encoder benchmark" hint="Tests every encoder found on synthetic frames, about 5 seconds.">
					<button class="secondary" disabled={benching || !inApp} onclick={runBench}>{benching ? 'Running…' : 'Compare encoders'}</button>
				</Row>
				{#if bench}
					<table class="bench">
						<tbody>
							{#each bench as b, i (i)}
								<tr>
									<td>{b.name}</td>
									<td class="num">{b.ok ? `${b.fps.toFixed(0)} fps` : 'Failed'}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				{/if}
				<Row label="Share settings" hint="A file a friend can import. Your clips folder isn't included.">
					<button class="secondary" disabled={!inApp} onclick={exportProfile}>Export</button>
					<button class="secondary" disabled={!inApp} onclick={importProfile}>Import</button>
				</Row>


				<h3 id="smart-highlights">Smart highlights (experimental)</h3>
				<p class="intro">Listens to a clip's audio for loud spikes, excited voices and long loud stretches, and marks them on the trim timeline. Runs on this PC only. Off, it never runs.</p>
				<Row label="Smart highlights" hint={s.smart_highlights.enabled ? 'Analyze clips from the library, one session at a time.' : 'Off. Nothing is analyzed.'}>
					<Toggle checked={s.smart_highlights.enabled} label="Smart highlights" onchange={(v) => set((x) => (x.smart_highlights.enabled = v))} />
				</Row>
				{#if s.smart_highlights.enabled}
					<Row label="Loudness threshold" hint="How loud a spike or stretch must get. Lower flags more moments." help="Measured in dBFS, where 0 is the loudest a recording can be. Game audio usually sits between -30 and -20; gunshots and explosions reach -15 to -5. Start at -14 and lower it if quiet games get no highlights.">
						<input type="range" min="-30" max="-3" step="1" value={s.smart_highlights.threshold_db} onchange={(e) => set((x) => (x.smart_highlights.threshold_db = num(e)))} aria-label="Loudness threshold" />
						<span class="readout num">{s.smart_highlights.threshold_db} dB</span>
					</Row>
					<Row label="Analyze on save" hint={s.smart_highlights.analyze_on_save ? 'Each clip is analyzed a moment after it saves, about a second of CPU per minute of clip.' : 'Manual only: use Analyze in the library.'}>
						<Toggle checked={s.smart_highlights.analyze_on_save} label="Analyze on save" onchange={(v) => set((x) => (x.smart_highlights.analyze_on_save = v))} />
					</Row>
				{/if}


				<h3 id="performance">Performance</h3>
				<PerformanceList />
				</details>
			</section>
		{/if}
	</div>
</div>

<style>
	.page {
		display: grid;
		grid-template-columns: 160px minmax(0, 720px);
		gap: 32px;
		padding: 32px;
	}
	@media (max-width: 799px) {
		.page {
			grid-template-columns: minmax(0, 1fr);
			padding: var(--s-4);
		}
		.index {
			display: none;
		}
	}
	.index {
		position: sticky;
		top: 0;
		align-self: start;
	}
	h1 {
		margin: 0 0 16px;
		font-size: 24px;
		font-weight: 650;
		letter-spacing: -0.02em;
	}
	.index ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 4px;
	}
	.index a {
		display: block;
		padding: 4px 0;
		color: var(--muted);
		text-decoration: none;
	}
	.index a:hover {
		color: var(--text);
	}
	/* Each section is a card: a raised, rounded container with room to breathe. */
	.pro,
	.body > section {
		padding: var(--s-4) var(--s-5);
		border-radius: var(--r-3);
		background: var(--raised);
		box-shadow: inset 0 0 0 1px var(--border);
	}
	.pro {
		margin-bottom: var(--s-4);
		padding-block: var(--s-1);
	}
	.body > section :global(.row + .row) {
		border-top: 1px solid var(--border);
	}
	.body > section :global(.row) {
		padding: var(--s-4) 0;
	}
	summary {
		list-style: none;
		cursor: pointer;
		display: flex;
		align-items: baseline;
		gap: 12px;
		padding: 8px 0;
	}
	summary::-webkit-details-marker {
		display: none;
	}
	summary::before {
		content: '';
		width: 6px;
		height: 6px;
		border-right: 1.5px solid var(--muted);
		border-bottom: 1.5px solid var(--muted);
		transform: translateY(-2px) rotate(-45deg);
		transition: transform 180ms ease-out;
		flex: none;
	}
	details[open] > summary::before {
		transform: translateY(-3px) rotate(45deg);
	}
	summary h2 {
		margin: 0;
	}
	summary span {
		color: var(--muted);
		font-size: 13px;
	}
	summary:focus-visible {
		outline: 2px solid var(--accent, currentColor);
		outline-offset: 2px;
	}
	@media (prefers-reduced-motion: reduce) {
		summary::before {
			transition: none;
		}
	}
	section {
		margin-bottom: var(--s-4);
		scroll-margin-top: var(--s-4);
	}
	h2 {
		margin: 0 0 var(--s-2);
		font-size: 15px;
		font-weight: 650;
		letter-spacing: -0.01em;
	}
	h3 {
		scroll-margin-top: 16px;
		margin: 24px 0 4px;
		font-size: 13px;
		font-weight: 600;
		color: var(--muted);
	}
	.block {
		padding: 12px 0 16px;
	}
	.storage-bar {
		padding: 8px 0 16px;
	}
	.block-label {
		font-weight: 500;
	}
	.block-hint {
		color: var(--muted);
		font-size: 13px;
		margin: 4px 0 12px;
	}
	.intro {
		color: var(--muted);
		font-size: 13px;
		margin: 0 0 8px;
		max-width: 60ch;
	}
	select,
	.numbox,
	input[type='text'] {
		height: 32px;
		padding: 0 8px;
		border: 0;
		border-radius: var(--r-1);
		background: var(--sunken);
		outline: none;
	}
	.numbox {
		width: 80px;
		text-align: right;
	}
	.unit {
		color: var(--faint);
	}
	.warn {
		margin: 0 0 8px;
		padding: 8px 12px;
		border-radius: 4px;
		background: var(--raised);
		box-shadow: inset 2px 0 0 var(--signal);
		max-width: 60ch;
	}
	.readout {
		width: 64px;
		text-align: right;
	}
	.readout.wide {
		width: auto;
	}
	input[type='range'] {
		width: 160px;
		accent-color: var(--text);
	}
	.secondary {
		height: 32px;
		padding: 0 12px;
		border: 0;
		border-radius: var(--r-1);
		background: var(--sunken);
		font-weight: 500;
		cursor: pointer;
	}
	.secondary:hover:not(:disabled) {
		background: var(--line);
	}
	.secondary:disabled {
		color: var(--faint);
		cursor: default;
	}
	.add {
		display: flex;
		gap: 8px;
		margin-top: 12px;
	}
	.add .grow {
		flex: 1;
		min-width: 0;
	}
	.chips {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}
	.chips li {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 4px 4px 4px 12px;
		border-radius: var(--r-1);
		background: var(--sunken);
	}
	.chips button {
		width: 24px;
		height: 24px;
		border: 0;
		background: transparent;
		color: var(--muted);
		cursor: pointer;
	}
	.chips button:hover {
		color: var(--text);
	}
	table {
		width: 100%;
		border-collapse: collapse;
	}
	td {
		padding: 4px 8px 4px 0;
	}
	.link {
		border: 0;
		background: none;
		color: var(--muted);
		cursor: pointer;
	}
	.link:hover {
		color: var(--text);
	}
	.bench td:last-child {
		text-align: right;
	}
</style>
