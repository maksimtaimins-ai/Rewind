<!--
  Settings → Profiles → Situational: settings that switch in by circumstance
  (a game is running, nothing is, one app has focus, battery, time of day).
  They sit under per-game profiles, which win where both set a value. With
  none switched on Rewind never checks the circumstances.
-->
<script lang="ts">
	import { slide } from 'svelte/transition';
	import { api, type Hotkeys, type SituationalProfile, type Settings, type Trigger } from '#lib/api.ts';
	import { app, inApp, updateSettings } from '#lib/stores/app.svelte.ts';
	import { ms, standard } from '#lib/motion.ts';
	import HotkeyInput from './HotkeyInput.svelte';
	import Row from './Row.svelte';
	import Toggle from './Toggle.svelte';

	const MAX = 12;
	const s = $derived(app.settings!);
	const set = (f: (x: Settings) => void) => updateSettings(f);
	const edit = (i: number, f: (p: SituationalProfile) => void) => set((x) => f(x.situational[i]));
	const val = (e: Event) => (e.currentTarget as HTMLInputElement | HTMLSelectElement).value;
	const optNum = (e: Event) => (val(e) === '' ? null : Number(val(e)));

	let open = $state<number | null>(null);
	let apps = $state<string[]>([]);
	async function loadApps() {
		if (inApp && !apps.length) apps = await api.runningApps();
	}

	const kinds: { kind: Trigger['kind']; label: string }[] = [
		{ kind: 'game_detected', label: 'A game is running' },
		{ kind: 'idle', label: 'No game is running' },
		{ kind: 'app_focused', label: 'An app has focus' },
		{ kind: 'on_battery', label: 'On battery' },
		{ kind: 'plugged_in', label: 'Plugged in' },
		{ kind: 'time_of_day', label: 'At certain hours' }
	];

	function trigger(kind: Trigger['kind']): Trigger {
		if (kind === 'app_focused') return { kind, exe: '' };
		if (kind === 'time_of_day') return { kind, from: '22:00', to: '06:00' };
		return { kind } as Trigger;
	}

	function when(t: Trigger): string {
		switch (t.kind) {
			case 'game_detected':
				return 'While a game is running';
			case 'idle':
				return 'While no game is running';
			case 'app_focused':
				return t.exe ? `While ${t.exe} has focus` : 'Pick an app';
			case 'on_battery':
				return 'While on battery';
			case 'plugged_in':
				return 'While plugged in';
			case 'time_of_day':
				return `Between ${t.from} and ${t.to}`;
		}
	}

	function summary(p: SituationalProfile): string {
		const parts: string[] = [];
		if (p.fps) parts.push(`${p.fps} fps`);
		if (p.height != null) parts.push(p.height ? `${p.height}p` : 'native');
		if (p.bitrate_kbps) parts.push(`${p.bitrate_kbps / 1000} Mb/s`);
		if (p.codec) parts.push(p.codec === 'hevc' ? 'HEVC' : 'H.264');
		if (p.buffer_seconds) parts.push(`${p.buffer_seconds}s replay`);
		if (p.hotkeys) parts.push('own hotkeys');
		if (p.overlays_off) parts.push('overlays off');
		return parts.length ? parts.join(', ') : 'Uses your usual settings';
	}

	const hotkeyRows: [keyof Hotkeys, string][] = [
		['save_clip', 'Save clip'],
		['marker', 'Mark this moment'],
		['screenshot', 'Screenshot'],
		['extend_last', 'Extend last clip'],
		['slow_mo', 'Slow-mo replay'],
		['favorite_last', 'Favorite last clip'],
		['compact_hud', 'Compact HUD']
	];

	function add() {
		set((x) =>
			x.situational.push({
				name: `Profile ${x.situational.length + 1}`,
				enabled: true,
				trigger: { kind: 'on_battery' },
				fps: 30, height: null, bitrate_kbps: null, codec: null, buffer_seconds: null, hotkeys: null, overlays_off: false
			})
		);
		open = s.situational.length - 1;
	}
</script>

<p class="intro">
	Switch settings by circumstance, on top of your usual ones. The first profile below whose condition holds is used; a game's own profile still wins where both
	set the same thing. Nothing here runs until you switch a profile on.
</p>

{#if s.situational.length}
	<ul class="list">
		{#each s.situational as p, i (i)}
			{@const live = app.status?.situation === p.name && p.enabled}
			<li>
				<div class="head">
					<div class="who">
						<span class="name">{p.name}{#if live}<span class="live">In use</span>{/if}</span>
						<span class="sub">{when(p.trigger)}. {summary(p)}</span>
					</div>
					<button class="secondary" aria-expanded={open === i} onclick={() => (open = open === i ? null : i)}>{open === i ? 'Done' : 'Edit'}</button>
					<Toggle checked={p.enabled} label="Use {p.name}" onchange={(v) => edit(i, (x) => (x.enabled = v))} />
				</div>
				{#if open === i}
					<div class="editor" transition:slide={{ duration: ms(180), easing: standard }}>
						<Row label="Name">
							<input type="text" value={p.name} onchange={(e) => edit(i, (x) => (x.name = val(e).trim() || x.name))} aria-label="Profile name" />
						</Row>
						<Row label="Active when">
							<select value={p.trigger.kind} onchange={(e) => edit(i, (x) => (x.trigger = trigger(val(e) as Trigger['kind'])))} aria-label="Active when">
								{#each kinds as k (k.kind)}<option value={k.kind}>{k.label}</option>{/each}
							</select>
							{#if p.trigger.kind === 'app_focused'}
								<input type="text" list="running-apps" placeholder="app.exe" value={p.trigger.exe} onfocus={loadApps} onchange={(e) => edit(i, (x) => (x.trigger = { kind: 'app_focused', exe: val(e).trim() }))} aria-label="App with focus" />
								<datalist id="running-apps">{#each apps as a (a)}<option value={a}></option>{/each}</datalist>
							{:else if p.trigger.kind === 'time_of_day'}
								{@const t = p.trigger}
								<input type="time" value={t.from} onchange={(e) => edit(i, (x) => (x.trigger = { kind: 'time_of_day', from: val(e) || t.from, to: t.to }))} aria-label="From" />
								<span class="to">to</span>
								<input type="time" value={t.to} onchange={(e) => edit(i, (x) => (x.trigger = { kind: 'time_of_day', from: t.from, to: val(e) || t.to }))} aria-label="To" />
							{/if}
						</Row>
						<Row label="Frame rate">
							<select value={p.fps ?? ''} onchange={(e) => edit(i, (x) => (x.fps = optNum(e)))} aria-label="Frame rate">
								<option value="">Usual ({s.video.fps})</option>
								{#each [24, 30, 60, 120, 144] as f (f)}<option value={f}>{f} fps</option>{/each}
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
						<Row label="Bitrate" hint="Mb/s. Empty keeps your usual {s.video.bitrate_kbps / 1000} Mb/s.">
							<input class="numbox num" type="number" min="1" max="200" placeholder={String(s.video.bitrate_kbps / 1000)} value={p.bitrate_kbps == null ? '' : p.bitrate_kbps / 1000} onchange={(e) => edit(i, (x) => (x.bitrate_kbps = val(e) === '' ? null : Math.round(Number(val(e)) * 1000)))} aria-label="Bitrate in Mb/s" />
						</Row>
						<Row label="Codec">
							<select value={p.codec ?? ''} onchange={(e) => edit(i, (x) => (x.codec = (val(e) || null) as typeof x.codec))} aria-label="Codec">
								<option value="">Usual ({s.video.codec === 'hevc' ? 'HEVC' : 'H.264'})</option>
								<option value="h264">H.264</option>
								<option value="hevc">HEVC</option>
							</select>
						</Row>
						<Row label="Replay length" hint="Seconds kept, slot A. Empty keeps your usual {s.buffer.seconds}s.">
							<input class="numbox num" type="number" min="10" max="300" placeholder={String(s.buffer.seconds)} value={p.buffer_seconds ?? ''} onchange={(e) => edit(i, (x) => (x.buffer_seconds = optNum(e)))} aria-label="Replay length in seconds" />
						</Row>
						<Row label="Overlays off" hint="Stops recording keystrokes and hides the live overlay while this is in force.">
							<Toggle checked={p.overlays_off} label="Overlays off" onchange={(v) => edit(i, (x) => (x.overlays_off = v))} />
						</Row>
						<Row label="Its own hotkeys" hint="Replaces every global hotkey while this is in force.">
							<Toggle checked={!!p.hotkeys} label="Own hotkeys" onchange={(v) => edit(i, (x) => (x.hotkeys = v ? { ...s.hotkeys } : null))} />
						</Row>
						{#if p.hotkeys}
							{#each hotkeyRows as [key, name] (key)}
								<Row label={name}>
									<HotkeyInput label="{name} in {p.name}" value={p.hotkeys[key]} onchange={(v) => edit(i, (x) => (x.hotkeys![key] = v))} />
								</Row>
							{/each}
						{/if}
						<div class="foot"><button class="link" onclick={() => (set((x) => x.situational.splice(i, 1)), (open = null))}>Delete profile</button></div>
					</div>
				{/if}
			</li>
		{/each}
	</ul>
{/if}
<div class="add"><button class="secondary" disabled={s.situational.length >= MAX} onclick={add}>Add a situational profile</button></div>

<style>
	.intro {
		color: var(--muted);
		font-size: 13px;
		margin: 0 0 12px;
		max-width: 62ch;
	}
	.list {
		list-style: none;
		margin: 0 0 12px;
		padding: 0;
		display: grid;
		gap: 8px;
	}
	li {
		border-radius: var(--r-2);
		background: var(--sunken);
		padding: 10px 12px;
	}
	.head {
		display: flex;
		align-items: center;
		gap: 12px;
	}
	.who {
		flex: 1;
		min-width: 0;
		display: grid;
	}
	.name {
		font-weight: 600;
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.live {
		font-size: 11px;
		font-weight: 500;
		padding: 1px 8px;
		border-radius: 10px;
		background: var(--accent);
		color: var(--on-accent);
	}
	.sub {
		color: var(--muted);
		font-size: 12px;
	}
	.editor {
		margin-top: 8px;
		border-top: 1px solid var(--border);
	}
	.editor :global(.row) {
		padding: 8px 0;
	}
	.to {
		color: var(--faint);
	}
	.foot {
		padding-top: 8px;
	}
	select,
	input[type='text'],
	input[type='time'],
	.numbox {
		height: 32px;
		padding: 0 8px;
		border: 0;
		border-radius: var(--r-1);
		background: var(--raised);
		outline: none;
	}
	.numbox {
		width: 80px;
		text-align: right;
	}
	.secondary {
		height: 32px;
		padding: 0 12px;
		border: 0;
		border-radius: var(--r-1);
		background: var(--raised);
		font-weight: 500;
		cursor: pointer;
	}
	.secondary:hover:not(:disabled) {
		background: var(--line);
	}
	.secondary:disabled {
		opacity: 0.4;
		cursor: default;
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
</style>
