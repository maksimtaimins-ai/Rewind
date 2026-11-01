<!-- Settings > Game profiles: settings that switch in while a game runs. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { slide } from 'svelte/transition';
	import { api, type GameProfile, type KeystrokePreset, type Settings } from '#lib/api.ts';
	import { app, inApp, updateSettings } from '#lib/stores/app.svelte.ts';
	import { mockKeystrokePresets } from '#lib/stores/mock.ts';
	import { ms, standard } from '#lib/motion.ts';
	import Row from './Row.svelte';
	import Toggle from './Toggle.svelte';
	import HotkeyInput from './HotkeyInput.svelte';

	const s = $derived(app.settings!);
	const set = (f: (x: Settings) => void) => updateSettings(f);
	const edit = (i: number, f: (p: GameProfile) => void) => set((x) => f(x.profiles[i]));
	const val = (e: Event) => (e.currentTarget as HTMLInputElement | HTMLSelectElement).value;
	const optNum = (e: Event) => (val(e) === '' ? null : Number(val(e)));

	let open = $state<string | null>(null);
	let presets = $state<KeystrokePreset[]>([]);
	let apps = $state<string[]>([]);
	let newExe = $state('');

	onMount(async () => {
		presets = inApp ? await api.keystrokePresets() : mockKeystrokePresets();
	});
	async function loadApps() {
		if (inApp && !apps.length) apps = await api.runningApps();
	}

	const exists = (exe: string) => s.profiles.some((p) => p.exe.toLowerCase() === exe.toLowerCase());
	const normalized = $derived(newExe.trim() && !/\.exe$/i.test(newExe.trim()) ? `${newExe.trim()}.exe` : newExe.trim());

	function add() {
		const exe = normalized;
		if (!exe || exists(exe)) return;
		const name = exe.replace(/\.exe$/i, '');
		set((x) =>
			x.profiles.push({
				exe, name, enabled: true, fps: null, height: null, bitrate_kbps: null, codec: null, buffer_seconds: null,
				slot_b_seconds: null, slot_c_seconds: null, hotkeys: null, subfolder: false, keystroke_preset: null, highlight: null
			})
		);
		newExe = '';
		open = exe;
	}

	// What a profile changes, in a few words.
	function summary(p: GameProfile): string {
		const parts: string[] = [];
		if (p.fps) parts.push(`${p.fps} fps`);
		if (p.height != null) parts.push(p.height ? `${p.height}p` : 'native');
		if (p.bitrate_kbps) parts.push(`${p.bitrate_kbps / 1000} Mb/s`);
		if (p.codec) parts.push(p.codec === 'hevc' ? 'HEVC' : 'H.264');
		if (p.buffer_seconds) parts.push(`${p.buffer_seconds}s replay`);
		if (p.hotkeys) parts.push('own hotkeys');
		if (p.subfolder) parts.push('own folder');
		if (p.keystroke_preset) parts.push(`${presets.find((x) => x.id === p.keystroke_preset)?.label ?? p.keystroke_preset} keys`);
		if (p.highlight) parts.push(p.highlight.color === 'off' ? 'no highlight' : `${p.highlight.color} highlight`);
		return parts.length ? parts.join(', ') : 'Uses your usual settings';
	}
</script>

<Row
	label="Switch settings per game"
	hint={s.profiles_enabled
		? app.status?.profile
			? `Using the ${app.status.profile} profile right now.`
			: 'When a game below is running, its profile replaces the settings it changes.'
		: 'Off. Profiles are kept but never applied.'}
>
	<Toggle checked={s.profiles_enabled} label="Switch settings per game" onchange={(v) => set((x) => (x.profiles_enabled = v))} />
</Row>

<ul class="list" class:off={!s.profiles_enabled}>
	{#each s.profiles as p, i (p.exe)}
		{@const active = s.profiles_enabled && app.status?.profile === p.name && p.enabled}
		<li class:open={open === p.exe}>
			<div class="head">
				<div class="who">
					<span class="name">
						{p.name}
						{#if active}<span class="live">In use</span>{/if}
					</span>
					<span class="sub"><span class="exe">{p.exe}</span> {summary(p)}</span>
				</div>
				<button class="secondary" aria-expanded={open === p.exe} onclick={() => (open = open === p.exe ? null : p.exe)}>
					{open === p.exe ? 'Done' : 'Edit profile'}
				</button>
				<Toggle checked={p.enabled} label="Use the {p.name} profile" onchange={(v) => edit(i, (x) => (x.enabled = v))} />
			</div>
			{#if open === p.exe}
				<div class="editor" transition:slide={{ duration: ms(180), easing: standard }}>
					<Row label="Name" hint="Also starts the file name of clips saved in this game.">
						<input type="text" value={p.name} onchange={(e) => edit(i, (x) => (x.name = val(e).trim() || x.name))} aria-label="Profile name" />
					</Row>
					<Row label="Frame rate">
						<select value={p.fps ?? ''} onchange={(e) => edit(i, (x) => (x.fps = optNum(e)))} aria-label="Frame rate">
							<option value="">Usual ({s.video.fps})</option>
							{#each [30, 60, 120, 144, 240] as f (f)}<option value={f}>{f} fps</option>{/each}
						</select>
					</Row>
					<Row label="Resolution">
						<select value={p.height ?? ''} onchange={(e) => edit(i, (x) => (x.height = optNum(e)))} aria-label="Resolution">
							<option value="">Usual ({s.video.height ? `${s.video.height}p` : 'native'})</option>
							<option value={720}>720p</option>
							<option value={1080}>1080p</option>
							<option value={1440}>1440p</option>
							<option value={0}>Native</option>
						</select>
					</Row>
					<Row label="Bitrate" hint="Mb/s. Empty uses your usual {s.video.bitrate_kbps / 1000} Mb/s.">
						<input class="numbox num" type="number" min="1" max="200" placeholder={String(s.video.bitrate_kbps / 1000)} value={p.bitrate_kbps == null ? '' : p.bitrate_kbps / 1000} onchange={(e) => edit(i, (x) => (x.bitrate_kbps = val(e) === '' ? null : Math.round(Number(val(e)) * 1000)))} aria-label="Bitrate in Mb/s" />
					</Row>
					<Row label="Codec">
						<select value={p.codec ?? ''} onchange={(e) => edit(i, (x) => (x.codec = (val(e) || null) as typeof x.codec))} aria-label="Codec">
							<option value="">Usual ({s.video.codec === 'hevc' ? 'HEVC' : 'H.264'})</option>
							<option value="h264">H.264</option>
							<option value="hevc">HEVC</option>
						</select>
					</Row>
					<Row label="Replay lengths" hint="Seconds for slots A, B and C. Empty uses your usual lengths.">
						{#each [['buffer_seconds', s.buffer.seconds, 'A'], ['slot_b_seconds', s.buffer.extra_slots[0]?.seconds, 'B'], ['slot_c_seconds', s.buffer.extra_slots[1]?.seconds, 'C']] as [key, usual, slot] (key)}
							{@const k = key as 'buffer_seconds' | 'slot_b_seconds' | 'slot_c_seconds'}
							<label class="slot"><span>{slot}</span><input class="numbox num" type="number" min="10" max="300" placeholder={String(usual)} value={p[k] ?? ''} onchange={(e) => edit(i, (x) => (x[k] = optNum(e)))} aria-label="Slot {slot} seconds" /></label>
						{/each}
					</Row>
					<Row label="Save to its own folder" hint={p.subfolder ? `Clips go to ${s.clips_dir}\\${p.name}. They still show in the library.` : 'Off: clips go to your clips folder.'}>
						<Toggle checked={p.subfolder} label="Own folder" onchange={(v) => edit(i, (x) => (x.subfolder = v))} />
					</Row>
					<Row label="Keystroke recording" hint={s.keystrokes.enabled ? 'Which keys to record while this game runs.' : 'Applies when keystroke recording is on (Capture).'}>
						<select value={p.keystroke_preset ?? ''} onchange={(e) => edit(i, (x) => (x.keystroke_preset = val(e) || null))} aria-label="Keystroke preset">
							<option value="">Usual</option>
							{#each presets as pr (pr.id)}<option value={pr.id}>{pr.label}</option>{/each}
						</select>
					</Row>
					<Row label="Recording highlight">
						<select
							value={p.highlight?.color ?? ''}
							onchange={(e) => edit(i, (x) => (x.highlight = val(e) ? { ...$state.snapshot(s.highlight), color: val(e) as 'off' } : null))}
							aria-label="Recording highlight"
						>
							<option value="">Usual</option>
							<option value="off">Off</option>
							<option value="blue">Blue</option>
							<option value="green">Green</option>
							<option value="red">Red</option>
						</select>
					</Row>
					<Row label="Own hotkeys" hint={p.hotkeys ? 'These replace your usual hotkeys while the game runs.' : 'Off: your usual hotkeys.'}>
						<Toggle checked={!!p.hotkeys} label="Own hotkeys" onchange={(v) => edit(i, (x) => (x.hotkeys = v ? { ...$state.snapshot(s.hotkeys) } : null))} />
					</Row>
					{#if p.hotkeys}
						{#each [['save_clip', 'Save clip'], ['marker', 'Mark this moment'], ['screenshot', 'Screenshot'], ['slow_mo', 'Slow-mo replay']] as [key, label] (key)}
							{@const k = key as 'save_clip' | 'marker' | 'screenshot' | 'slow_mo'}
							<Row {label}>
								<HotkeyInput {label} value={p.hotkeys[k]} onchange={(v) => edit(i, (x) => (x.hotkeys![k] = v))} />
							</Row>
						{/each}
					{/if}
					<button class="link" onclick={() => ((open = null), set((x) => x.profiles.splice(i, 1)))}>Remove {p.name}</button>
				</div>
			{/if}
		</li>
	{/each}
</ul>

<form class="add" onsubmit={(e) => (e.preventDefault(), add())}>
	<input type="text" list="running-apps" placeholder="Game, e.g. eldenring.exe" bind:value={newExe} onfocus={loadApps} aria-label="Game executable" />
	<datalist id="running-apps">
		{#each apps as a (a)}<option value={a}></option>{/each}
	</datalist>
	<button class="secondary" type="submit" disabled={!normalized || exists(normalized)}>Add game</button>
</form>
<p class="note">Start typing to pick from running apps. Rewind checks which game is running every couple of seconds for clip names already; profiles only add the switch.</p>

<style>
	.list {
		list-style: none;
		margin: 8px 0 0;
		padding: 0;
		transition: opacity 180ms var(--ease);
	}
	.list.off {
		opacity: 0.55;
	}
	li {
		box-shadow: 0 1px 0 var(--line);
	}
	.head {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 10px 0;
	}
	.who {
		flex: 1;
		min-width: 0;
		display: grid;
		gap: 2px;
	}
	.name {
		font-weight: 500;
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.live {
		padding: 1px 6px;
		border-radius: 3px;
		background: var(--text);
		color: var(--ground);
		font-size: 11px;
		font-weight: 600;
	}
	.sub {
		color: var(--muted);
		font-size: 13px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.exe {
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--faint);
		margin-right: 6px;
	}
	.editor {
		margin: 0 0 12px;
		padding: 0 0 4px 16px;
		box-shadow: inset 1px 0 0 var(--line-strong);
	}
	.slot {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		color: var(--faint);
	}
	select,
	.numbox,
	input[type='text'] {
		height: 32px;
		padding: 0 8px;
		border: 0;
		border-radius: 4px;
		background: var(--sunken);
		outline: none;
	}
	.numbox {
		width: 64px;
		text-align: right;
	}
	.add {
		display: flex;
		gap: 8px;
		margin-top: 12px;
	}
	.add input {
		flex: 1;
		max-width: 320px;
	}
	.note {
		margin: 8px 0 0;
		color: var(--faint);
		font-size: 12px;
		max-width: 60ch;
	}
	.secondary {
		height: 32px;
		padding: 0 12px;
		border: 0;
		border-radius: 4px;
		background: var(--raised);
		font-weight: 500;
		cursor: pointer;
		flex: none;
	}
	.secondary:hover:not(:disabled) {
		background: var(--line);
	}
	.secondary:disabled {
		color: var(--faint);
		cursor: default;
	}
	.link {
		margin: 8px 0;
		border: 0;
		background: none;
		padding: 0;
		color: var(--muted);
		cursor: pointer;
	}
	.link:hover {
		color: var(--text);
	}
</style>
