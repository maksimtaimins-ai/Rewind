<script lang="ts">
	import { onMount } from 'svelte';
	import { open, save } from '@tauri-apps/plugin-dialog';
	import { api, type BenchResult, type Device, type EncoderInfo, type Monitor, type Settings, type AudioSource } from '#lib/api.ts';
	import { app, inApp, toast, updateSettings } from '#lib/stores/app.svelte.ts';
	import Row from '#lib/components/Row.svelte';
	import Toggle from '#lib/components/Toggle.svelte';
	import Segmented from '#lib/components/Segmented.svelte';
	import HotkeyInput from '#lib/components/HotkeyInput.svelte';
	import ThemePicker from '#lib/components/ThemePicker.svelte';

	const s = $derived(app.settings!);
	const pro = $derived(s.pro_mode);

	let monitors = $state<Monitor[]>([]);
	let encoders = $state<EncoderInfo[]>([]);
	let mics = $state<Device[]>([]);
	let apps = $state<string[]>([]);
	let bench = $state<BenchResult[] | null>(null);
	let benching = $state(false);
	let newPrivacy = $state('');
	let newProfile = $state('');

	onMount(async () => {
		if (!inApp) {
			monitors = [{ index: 0, name: 'DISPLAY1', width: 2560, height: 1440, primary: true }];
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

	const sections = $derived(
		[
			['general', 'General'],
			['recording', 'Replay buffer'],
			['video', 'Video'],
			['audio', 'Audio'],
			['hotkeys', 'Hotkeys'],
			['games', 'Games'],
			['privacy', 'Privacy'],
			['storage', 'Storage']
		].filter(([id]) => pro || !['games', 'storage'].includes(id))
	);

	const kinds = $derived([...new Set(encoders.filter((e) => e.codec === s.video.codec).map((e) => e.kind))]);
	const kindLabel: Record<string, string> = {
		auto: 'Best available',
		nvenc: 'NVIDIA NVENC',
		amf: 'AMD AMF',
		qsv: 'Intel Quick Sync',
		software: 'CPU (slow)'
	};

	// Rough RAM use of the encoded buffer, to make the cost of a long buffer visible.
	const ramEstimate = $derived(
		Math.round(((s.video.bitrate_kbps * 1000) / 8 / 1e6) * s.buffer.seconds)
	);

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

	async function loadApps() {
		if (inApp && !apps.length) apps = await api.runningApps();
	}
</script>

<div class="page">
	<aside class="index">
		<h1>Settings</h1>
		<ul>
			{#each sections as [id, label] (id)}
				<li><a href="#{id}">{label}</a></li>
			{/each}
		</ul>
		<label class="pro">
			<Toggle checked={pro} label="Show advanced settings" onchange={(v) => set((x) => (x.pro_mode = v))} />
			<span>Advanced</span>
		</label>
	</aside>

	<div class="body">
		<section id="general">
			<h2>General</h2>
			<div class="block">
				<div class="block-label">Theme</div>
				<ThemePicker value={s.theme} onchange={(v) => set((x) => (x.theme = v))} />
			</div>
			<Row label="Launch when Windows starts" hint="Starts in the tray with the replay buffer running.">
				<Toggle checked={s.launch_on_startup} label="Launch when Windows starts" onchange={(v) => set((x) => (x.launch_on_startup = v))} />
			</Row>
			<Row label="Open minimized to the tray">
				<Toggle checked={s.start_minimized} label="Open minimized" onchange={(v) => set((x) => (x.start_minimized = v))} />
			</Row>
			<Row label="Play a sound when a clip is saved">
				<Toggle checked={s.play_sound_on_save} label="Sound on save" onchange={(v) => set((x) => (x.play_sound_on_save = v))} />
			</Row>
		</section>

		<section id="recording">
			<h2>Replay buffer</h2>
			<Row label="Buffer length" hint="A saved clip is the last this-many seconds before you press the hotkey, 10 to 300 seconds. The buffer runs whenever Rewind is open. About {ramEstimate} MB of RAM at this bitrate.">
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
			<Row label="Extend last clip by" hint="Footage added before the start of your last clip, if the buffer still has it.">
				<input class="numbox num" type="number" min="5" max="120" value={s.buffer.extend_secs} onchange={(e) => set((x) => (x.buffer.extend_secs = Math.min(120, Math.max(5, num(e)))))} aria-label="Extend seconds" />
				<span class="unit">s</span>
			</Row>
			{#if pro}
				<Row label="Memory limit" hint="Oldest seconds are dropped first if the buffer would exceed this.">
					<input class="numbox num" type="number" min="128" max="16384" step="128" value={s.buffer.max_memory_mb} onchange={(e) => set((x) => (x.buffer.max_memory_mb = num(e)))} aria-label="Memory limit in MB" />
					<span class="unit">MB</span>
				</Row>
			{/if}
			<Row label="Save clips to" hint={s.clips_dir}>
				<button class="secondary" onclick={pickFolder}>Change</button>
			</Row>
		</section>

		<section id="video">
			<h2>Video</h2>
			{#if monitors.length > 1}
				<Row label="Screen">
					<select value={s.video.monitor} onchange={(e) => set((x) => (x.video.monitor = num(e)))} aria-label="Screen">
						{#each monitors as m (m.index)}
							<option value={m.index}>{m.name}, {m.width}×{m.height}{m.primary ? ' (main)' : ''}</option>
						{/each}
					</select>
				</Row>
			{/if}
			<Row label="Resolution" hint="Downscaling happens on the GPU.">
				<Segmented
					label="Resolution"
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
			<Row label="Frame rate" hint="A cap. Variable refresh is followed as it happens.">
				<Segmented
					label="Frame rate"
					value={s.video.fps}
					options={[30, 60, 120, 144].map((f) => ({ value: f, label: String(f) }))}
					onchange={(v) => set((x) => (x.video.fps = v))}
				/>
			</Row>
			<Row label="Bitrate" hint="Higher keeps fast motion sharp and makes bigger files.">
				<input
					type="range"
					min="2000"
					max="80000"
					step="1000"
					value={s.video.bitrate_kbps}
					onchange={(e) => set((x) => (x.video.bitrate_kbps = num(e)))}
					aria-label="Bitrate"
				/>
				<span class="readout num">{(s.video.bitrate_kbps / 1000).toFixed(0)} Mb/s</span>
			</Row>
			<Row label="Encoder" hint="Hardware encoders keep the CPU free for the game.">
				<select value={s.video.encoder} onchange={(e) => set((x) => (x.video.encoder = (e.currentTarget as HTMLSelectElement).value as Settings['video']['encoder']))} aria-label="Encoder">
					<option value="auto">{kindLabel.auto}</option>
					{#each kinds as k (k)}
						<option value={k}>{kindLabel[k] ?? k}</option>
					{/each}
				</select>
			</Row>
			<Row label="Record the mouse cursor">
				<Toggle checked={s.video.capture_cursor} label="Record cursor" onchange={(v) => set((x) => (x.video.capture_cursor = v))} />
			</Row>
			{#if pro}
				<Row label="Codec" hint="HEVC makes smaller files; some editors and Discord on older devices can't play it.">
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
				<Row label="Rate control">
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
					<Row label="Quality" hint="1 to 100.">
						<input class="numbox num" type="number" min="1" max="100" value={s.video.quality} onchange={(e) => set((x) => (x.video.quality = num(e)))} aria-label="Quality" />
					</Row>
				{/if}
				<Row label="Keyframe every" hint="Clips and trims start on keyframes. Shorter is more precise, slightly larger.">
					<Segmented
						label="Keyframe interval"
						value={s.video.keyframe_secs}
						options={[0.5, 1, 2].map((v) => ({ value: v, label: `${v}s` }))}
						onchange={(v) => set((x) => (x.video.keyframe_secs = v))}
					/>
				</Row>
				<Row label="Encoder benchmark" hint="Encodes test frames on every encoder found, about 5 seconds.">
					<button class="secondary" disabled={benching || !inApp} onclick={runBench}>{benching ? 'Running…' : 'Run'}</button>
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
			{/if}
		</section>

		<section id="audio">
			<h2>Audio</h2>
			<p class="intro">Each source is its own track in the file, so editors can mix them separately. Players play the first one.</p>
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
						Add “everything except Discord”
					</button>
				</div>
			{/if}
		</section>

		<section id="hotkeys">
			<h2>Hotkeys</h2>
			<Row label="Save clip" hint="Saves the last {s.buffer.seconds} seconds.">
				<HotkeyInput label="Save clip" value={s.hotkeys.save_clip} onchange={(v) => set((x) => (x.hotkeys.save_clip = v))} />
			</Row>
			<Row label="Mark this moment" hint="Decide how much to keep later, from the Live tab.">
				<HotkeyInput label="Marker" value={s.hotkeys.marker} onchange={(v) => set((x) => (x.hotkeys.marker = v))} />
			</Row>
			<Row label="Screenshot">
				<HotkeyInput label="Screenshot" value={s.hotkeys.screenshot} onchange={(v) => set((x) => (x.hotkeys.screenshot = v))} />
			</Row>
			<Row label="Extend last clip" hint="Adds {s.buffer.extend_secs} seconds before the start of your last clip and replaces it.">
				<HotkeyInput label="Extend last clip" value={s.hotkeys.extend_last} onchange={(v) => set((x) => (x.hotkeys.extend_last = v))} />
			</Row>
		</section>

		{#if pro}
			<section id="games">
				<h2>Games</h2>
				<p class="intro">Per-game overrides apply while that game is running. Empty fields use the settings above.</p>
				{#if s.profiles.length}
					<table class="profiles">
						<thead>
							<tr><th>Game</th><th>FPS</th><th>Height</th><th>Mb/s</th><th>Replay s</th><th></th></tr>
						</thead>
						<tbody>
							{#each s.profiles as p, i (p.exe)}
								<tr>
									<td><input value={p.name} onchange={(e) => set((x) => (x.profiles[i].name = (e.currentTarget as HTMLInputElement).value))} aria-label="Name for {p.exe}" /><span class="exe">{p.exe}</span></td>
									{#each [['fps', 1], ['height', 1], ['bitrate_kbps', 1000], ['buffer_seconds', 1]] as [key, scale] (key)}
										{@const k = key as 'fps' | 'height' | 'bitrate_kbps' | 'buffer_seconds'}
										<td>
											<input
												class="num"
												type="number"
												placeholder="—"
												value={p[k] == null ? '' : p[k]! / (scale as number)}
												onchange={(e) => {
													const v = (e.currentTarget as HTMLInputElement).value;
													set((x) => (x.profiles[i][k] = v === '' ? null : Math.round(Number(v) * (scale as number))));
												}}
												aria-label="{k} for {p.name}"
											/>
										</td>
									{/each}
									<td><button class="link" onclick={() => set((x) => x.profiles.splice(i, 1))}>Remove</button></td>
								</tr>
							{/each}
						</tbody>
					</table>
				{/if}
				<div class="add">
					<select bind:value={newProfile} onfocus={loadApps} aria-label="Running app to add">
						<option value="">Pick a running game…</option>
						{#each apps as a (a)}<option value={a}>{a}</option>{/each}
					</select>
					<button
						class="secondary"
						disabled={!newProfile || s.profiles.some((p) => p.exe.toLowerCase() === newProfile.toLowerCase())}
						onclick={() => {
							const exe = newProfile;
							set((x) => x.profiles.push({ exe, name: exe.replace(/\.exe$/i, ''), fps: null, height: null, bitrate_kbps: null, buffer_seconds: null }));
							newProfile = '';
						}}>Add profile</button
					>
				</div>
			</section>
		{/if}

		<section id="privacy">
			<h2>Privacy</h2>
			<p class="intro">
				Rewind has no account, no cloud and no telemetry. Clips are written only to the folder you choose. Video
				capture pauses whenever one of these apps is focused.
			</p>
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
		</section>

		{#if pro}
			<section id="storage">
				<h2>Storage</h2>
				<Row label="Warn after writing" hint="Daily total of clips written, to spare your SSD. {app.status?.written_today_gb.toFixed(2) ?? 0} GB so far today.">
					<input class="numbox num" type="number" min="0" max="2000" value={s.ssd_warn_gb_per_day} onchange={(e) => set((x) => (x.ssd_warn_gb_per_day = num(e)))} aria-label="Daily write warning in GB" />
					<span class="unit">GB</span>
				</Row>
				<Row label="Share settings" hint="Export to a file a friend can import. Your clips folder isn't included.">
					<button class="secondary" disabled={!inApp} onclick={exportProfile}>Export</button>
					<button class="secondary" disabled={!inApp} onclick={importProfile}>Import</button>
				</Row>
			</section>
		{/if}
	</div>
</div>

<style>
	.page {
		display: grid;
		grid-template-columns: 176px minmax(0, 720px);
		gap: 32px;
		padding: 16px 32px 32px;
	}
	.index {
		position: sticky;
		top: 0;
		align-self: start;
	}
	h1 {
		margin: 0 0 16px;
		font-size: 32px;
		font-weight: 650;
		letter-spacing: -0.02em;
	}
	.index ul {
		list-style: none;
		margin: 0 0 24px;
		padding: 0;
		display: grid;
		gap: 4px;
	}
	.index a {
		color: var(--muted);
		text-decoration: none;
	}
	.index a:hover {
		color: var(--text);
	}
	.pro {
		display: flex;
		align-items: center;
		gap: 8px;
		color: var(--muted);
		cursor: pointer;
	}
	section {
		padding-bottom: 32px;
		scroll-margin-top: 16px;
	}
	h2 {
		margin: 0 0 8px;
		font-size: 18px;
		font-weight: 600;
		letter-spacing: -0.01em;
	}
	.block {
		padding: 12px 0 16px;
	}
	.block-label {
		font-weight: 500;
		margin-bottom: 12px;
	}
	.intro {
		color: var(--muted);
		margin: 0 0 12px;
		max-width: 60ch;
	}
	select,
	.numbox,
	input[type='text'] {
		height: 32px;
		padding: 0 8px;
		border: 1px solid var(--line);
		border-radius: 4px;
		background: var(--ground);
		outline: none;
	}
	select:focus,
	.numbox:focus,
	input[type='text']:focus {
		border-color: var(--muted);
	}
	.numbox {
		width: 80px;
		text-align: right;
	}
	.unit {
		color: var(--faint);
	}
	.readout {
		width: 64px;
		text-align: right;
	}
	input[type='range'] {
		width: 160px;
		accent-color: var(--accent);
	}
	.secondary {
		height: 32px;
		padding: 0 12px;
		border: 0;
		border-radius: 4px;
		background: var(--raised);
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
		border-radius: 4px;
		background: var(--raised);
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
	th {
		text-align: left;
		font-weight: 500;
		color: var(--faint);
		font-size: 12px;
		padding: 8px 8px 8px 0;
	}
	td {
		padding: 8px 8px 8px 0;
		border-top: 1px solid var(--line);
	}
	.profiles input {
		width: 100%;
		height: 28px;
		border: 1px solid transparent;
		border-radius: 4px;
		background: transparent;
		padding: 0 4px;
	}
	.profiles input:hover,
	.profiles input:focus {
		border-color: var(--line);
		outline: none;
	}
	.profiles td:not(:first-child) {
		width: 72px;
	}
	.exe {
		display: block;
		color: var(--faint);
		font-size: 12px;
		padding-left: 4px;
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
