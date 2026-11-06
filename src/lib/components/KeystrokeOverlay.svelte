<!-- Settings > Capture > Keystroke overlay: what to record, how it looks in exports, where it sits. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { api, type KeystrokePreset, type Keystrokes, type Settings } from '#lib/api.ts';
	import { app, inApp, updateSettings } from '#lib/stores/app.svelte.ts';
	import { mockKeystrokePresets } from '#lib/stores/mock.ts';
	import { keyName } from '#lib/overlay.ts';
	import Row from './Row.svelte';
	import Toggle from './Toggle.svelte';
	import Segmented from './Segmented.svelte';
	import OverlayPreview from './OverlayPreview.svelte';

	const k = $derived(app.settings!.keystrokes);
	const set = (f: (x: Keystrokes) => void) => updateSettings((s: Settings) => f(s.keystrokes));
	const num = (e: Event) => Number((e.currentTarget as HTMLInputElement).value);
	const str = (e: Event) => (e.currentTarget as HTMLInputElement | HTMLSelectElement).value;

	let presets = $state<KeystrokePreset[]>([]);
	onMount(async () => {
		presets = inApp ? await api.keystrokePresets() : mockKeystrokePresets();
	});

	function turnOn(on: boolean, live = false) {
		set((x) => {
			if (live) x.live = on;
			else x.enabled = on;
			// The keycaps start in the current theme's accent.
			if (on && !x.style.base_color) {
				const accent = getComputedStyle(document.documentElement).getPropertyValue('--accent').trim();
				if (/^#[0-9a-f]{6}$/i.test(accent)) x.style.base_color = accent;
			}
		});
	}

	function applyPreset(p: KeystrokePreset) {
		set((x) => {
			x.filter = p.filter;
			x.keys = [...p.keys];
			x.keyboard = p.keyboard;
			x.clicks = p.clicks;
			x.scroll = p.scroll;
			x.preset = p.id;
		});
	}
	// Any hand edit of the filters makes it a custom setup.
	const custom = (f: (x: Keystrokes) => void) => set((x) => (f(x), (x.preset = 'custom')));

	let listening = $state(false);
	function capture(e: KeyboardEvent) {
		if (!listening) return;
		e.preventDefault();
		e.stopPropagation();
		listening = false;
		// Escape cancels; Esc itself is in the quick list below.
		if (e.code === 'Escape') return;
		const name = keyName(e);
		if (name && !k.keys.includes(name)) custom((x) => x.keys.push(name));
	}
	const common = ['W', 'A', 'S', 'D', 'Space', 'Shift', 'Ctrl', 'Q', 'E', 'R', 'F', 'Tab', 'Esc', 'Up', 'Down', 'Left', 'Right'];
</script>

<svelte:window onkeydown={capture} />

<h3>Keystroke overlay</h3>
<div class="privacy" role="note">
	<strong>Keystroke recording captures what you type.</strong>
	Do not use it with password entry visible on screen. Your privacy apps are still honored: nothing is recorded while one of them is focused.
</div>
<Row label="Record keystrokes" hint={k.enabled ? 'Saved with each clip. Draw them in with Export > Draw keystrokes.' : 'Off. No keyboard or mouse hook is installed for clips.'}>
	<Toggle checked={k.enabled} label="Record keystrokes" onchange={(v) => turnOn(v)} />
</Row>
<Row
	label="Show on screen (live)"
	hint={k.live
		? 'Recent keys drawn over your screen as you press them, in the style below. Left out of clips and screenshots. Enables the global keyboard hook (~0% CPU).'
		: 'Off. Enables the global keyboard hook (~0% CPU) when on.'}
>
	<Toggle checked={k.live} label="Show keystrokes on screen" onchange={(v) => turnOn(v, true)} />
</Row>

{#if k.enabled || k.live}
	<div class="group">
		<h4>{k.enabled ? 'What to record' : 'What to show'}</h4>
		<div class="presets" role="group" aria-label="Presets">
			{#each presets as p (p.id)}
				<button class="preset" class:on={k.preset === p.id} aria-pressed={k.preset === p.id} onclick={() => applyPreset(p)}>{p.label}</button>
			{/each}
			<span class="preset static" class:on={k.preset === 'custom'}>Custom</span>
		</div>
		<Row label="Keyboard">
			<Toggle checked={k.keyboard} label="Record keyboard" onchange={(v) => custom((x) => (x.keyboard = v))} />
		</Row>
		<Row label="Mouse clicks">
			<Toggle checked={k.clicks} label="Record mouse clicks" onchange={(v) => custom((x) => (x.clicks = v))} />
		</Row>
		<Row label="Scroll wheel">
			<Toggle checked={k.scroll} label="Record scroll wheel" onchange={(v) => custom((x) => (x.scroll = v))} />
		</Row>
		<Row label="Mouse movement" hint={k.movement ? 'About 60 positions a second: input files get much larger.' : 'Needed for the pointer trail. Makes input files much larger.'}>
			<Toggle checked={k.movement} label="Record mouse movement" onchange={(v) => custom((x) => (x.movement = v))} />
		</Row>
		<Row label="Pause while typing in a text box" hint="Skips keys whenever a text field anywhere has the blinking cursor: chat, browsers, sign-in forms.">
			<Toggle checked={k.pause_in_text_fields} label="Pause in text fields" onchange={(v) => set((x) => (x.pause_in_text_fields = v))} />
		</Row>
		<Row label="Keys" hint={k.filter === 'all' ? 'Every key.' : k.filter === 'allow' ? 'Only the keys below. They show as a mini keyboard.' : 'Every key except the ones below.'}>
			<Segmented
				label="Key filter"
				value={k.filter}
				options={[
					{ value: 'all', label: 'All keys' },
					{ value: 'allow', label: 'Only these' },
					{ value: 'block', label: 'All except' }
				]}
				onchange={(v) => custom((x) => (x.filter = v))}
			/>
		</Row>
		{#if k.filter !== 'all'}
			<div class="keys">
				{#each k.keys as key, i (key)}
					<span class="key">{key}<button aria-label="Remove {key}" onclick={() => custom((x) => x.keys.splice(i, 1))}>×</button></span>
				{/each}
				<button class="listen" class:on={listening} onclick={() => (listening = !listening)} onblur={() => (listening = false)}>
					{listening ? 'Press a key…' : '+ Add key'}
				</button>
			</div>
			<div class="quick" aria-label="Common keys">
				{#each common.filter((c) => !k.keys.includes(c)) as c (c)}
					<button onclick={() => custom((x) => x.keys.push(c))}>{c}</button>
				{/each}
			</div>
		{/if}
	</div>

	<div class="group">
		<h4>Look</h4>
		<p class="intro">
			{k.live ? 'Used on screen and in exported copies.' : 'Drawn into exported copies, never on your screen while you play.'} Drag the preview to place it.
		</p>
		<OverlayPreview cfg={k} onmove={(x, y) => set((s) => ((s.position.anchor = 'custom'), (s.position.x = x), (s.position.y = y)))} />
		<Row label="Keycap">
			<Segmented
				label="Keycap style"
				value={k.style.keycap}
				options={[
					{ value: 'flat', label: 'Flat' },
					{ value: 'three_d', label: '3D' },
					{ value: 'minimal', label: 'Minimal' },
					{ value: 'outlined', label: 'Outlined' },
					{ value: 'mechanical', label: 'Mechanical' }
				]}
				onchange={(v) => set((x) => (x.style.keycap = v))}
			/>
		</Row>
		<Row label="Size" hint="Keycap height at 1080p; scales with the export.">
			<input type="range" min="16" max="120" step="2" value={k.style.size} onchange={(e) => set((x) => (x.style.size = num(e)))} aria-label="Keycap size" />
			<span class="readout num">{k.style.size}px</span>
		</Row>
		<Row label="Font">
			<select value={k.style.font} onchange={(e) => set((x) => (x.style.font = str(e) as typeof x.style.font))} aria-label="Font">
				<option value="theme">App font</option>
				<option value="sans">Sans</option>
				<option value="mono">Monospace</option>
				<option value="condensed">Condensed</option>
			</select>
		</Row>
		<Row label="Key color">
			<input type="color" value={k.style.base_color || '#e7e7e5'} onchange={(e) => set((x) => (x.style.base_color = str(e)))} aria-label="Key color" />
		</Row>
		<Row label="Text color" hint={k.style.text_color ? 'Fixed.' : 'Automatic: black or white, whichever reads better.'}>
			{#if k.style.text_color}
				<input type="color" value={k.style.text_color} onchange={(e) => set((x) => (x.style.text_color = str(e)))} aria-label="Text color" />
			{/if}
			<Segmented
				label="Text color"
				value={k.style.text_color ? 'pick' : 'auto'}
				options={[
					{ value: 'auto', label: 'Auto' },
					{ value: 'pick', label: 'Pick' }
				]}
				onchange={(v) => set((x) => (x.style.text_color = v === 'auto' ? '' : '#ffffff'))}
			/>
		</Row>
		<Row label="Border">
			{#if k.style.border}
				<input type="color" value={k.style.border_color} onchange={(e) => set((x) => (x.style.border_color = str(e)))} aria-label="Border color" />
			{/if}
			<Toggle checked={k.style.border} label="Border" onchange={(v) => set((x) => (x.style.border = v))} />
		</Row>
		<Row label="Corner radius">
			<input type="range" min="0" max="40" step="1" value={k.style.radius} onchange={(e) => set((x) => (x.style.radius = num(e)))} aria-label="Corner radius" />
			<span class="readout num">{k.style.radius}px</span>
		</Row>
		<Row label="Shadow">
			{#if k.style.shadow}
				<input type="range" min="0" max="100" step="5" value={k.style.shadow_intensity} onchange={(e) => set((x) => (x.style.shadow_intensity = num(e)))} aria-label="Shadow intensity" />
				<span class="readout num">{k.style.shadow_intensity}%</span>
			{/if}
			<Toggle checked={k.style.shadow} label="Shadow" onchange={(v) => set((x) => (x.style.shadow = v))} />
		</Row>
		<Row label="Press animation">
			<select value={k.style.animation} onchange={(e) => set((x) => (x.style.animation = str(e) as typeof x.style.animation))} aria-label="Press animation">
				<option value="none">None</option>
				<option value="pop">Scale pop</option>
				<option value="flash">Flash</option>
				<option value="spring">Spring bounce</option>
				<option value="pulse">Pulse while held</option>
			</select>
		</Row>
		<Row label="Fade out" hint="How long a released key stays visible.">
			<input type="range" min="0.5" max="10" step="0.5" value={k.style.fade_secs} onchange={(e) => set((x) => (x.style.fade_secs = num(e)))} aria-label="Fade out" />
			<span class="readout num">{k.style.fade_secs}s</span>
		</Row>
		<Row label="Opacity when pressed">
			<input type="range" min="0" max="100" step="5" value={k.style.pressed_opacity} onchange={(e) => set((x) => (x.style.pressed_opacity = num(e)))} aria-label="Opacity when pressed" />
			<span class="readout num">{k.style.pressed_opacity}%</span>
		</Row>
		<Row label="Opacity at rest" hint="For the mini keyboard (Only these): keys not being pressed.">
			<input type="range" min="0" max="100" step="5" value={k.style.idle_opacity} onchange={(e) => set((x) => (x.style.idle_opacity = num(e)))} aria-label="Opacity at rest" />
			<span class="readout num">{k.style.idle_opacity}%</span>
		</Row>
		<Row label="Show held keys filled" hint="Held keys are solid; released ones turn to an outline as they fade.">
			<Toggle checked={k.style.hold_indicator} label="Hold indicator" onchange={(v) => set((x) => (x.style.hold_indicator = v))} />
		</Row>
		<Row label="Key combos">
			<Segmented
				label="Key combos"
				value={k.style.combo}
				options={[
					{ value: 'chain', label: 'Ctrl + Shift + P' },
					{ value: 'stack', label: 'Stacked' }
				]}
				onchange={(v) => set((x) => (x.style.combo = v))}
			/>
		</Row>
	</div>

	<div class="group">
		<h4>Position</h4>
		<Row label="Place">
			<select value={k.position.anchor} onchange={(e) => set((x) => (x.position.anchor = str(e) as typeof x.position.anchor))} aria-label="Overlay position">
				<option value="top_left">Top left</option>
				<option value="top_center">Top center</option>
				<option value="top_right">Top right</option>
				<option value="bottom_left">Bottom left</option>
				<option value="bottom_center">Bottom center</option>
				<option value="bottom_right">Bottom right</option>
				<option value="custom">Custom (drag the preview)</option>
			</select>
		</Row>
		{#if k.position.anchor !== 'custom'}
			<Row label="Distance from edge" hint="At 1080p.">
				<input type="range" min="0" max="200" step="4" value={k.position.margin} onchange={(e) => set((x) => (x.position.margin = num(e)))} aria-label="Distance from edge" />
				<span class="readout num">{k.position.margin}px</span>
			</Row>
		{/if}
		{#if k.clicks}
			<Row label="Mouse clicks show">
				<Segmented
					label="Click position"
					value={k.position.click_position}
					options={[
						{ value: 'cursor', label: 'At the pointer' },
						{ value: 'with_keys', label: 'As keys' }
					]}
					onchange={(v) => set((x) => (x.position.click_position = v))}
				/>
			</Row>
			{#if k.position.click_position === 'cursor'}
				<Row label="Click marker">
					<Segmented
						label="Click marker"
						value={k.position.click_style}
						options={[
							{ value: 'dot', label: 'Dot' },
							{ value: 'ring', label: 'Ring' },
							{ value: 'flash', label: 'Flash' }
						]}
						onchange={(v) => set((x) => (x.position.click_style = v))}
					/>
				</Row>
			{/if}
		{/if}
		<Row label="Pointer trail" hint={k.movement ? 'A fading line behind the pointer.' : 'Needs Mouse movement recorded.'}>
			{#if k.position.trail}
				<input type="range" min="100" max="2000" step="100" value={k.position.trail_ms} onchange={(e) => set((x) => (x.position.trail_ms = num(e)))} aria-label="Trail length" />
				<span class="readout num">{(k.position.trail_ms / 1000).toFixed(1)}s</span>
			{/if}
			<Toggle checked={k.position.trail} label="Pointer trail" onchange={(v) => set((x) => (x.position.trail = v))} />
		</Row>
	</div>
{/if}

<style>
	h3 {
		margin: 24px 0 8px;
		font-size: 13px;
		font-weight: 600;
		color: var(--muted);
	}
	h4 {
		margin: 0 0 4px;
		font-size: 13px;
		font-weight: 600;
	}
	.privacy {
		margin: 0 0 8px;
		padding: 10px 14px;
		border-radius: 4px;
		background: var(--raised);
		box-shadow: inset 3px 0 0 var(--signal);
		max-width: 64ch;
		font-size: 13px;
		color: var(--muted);
	}
	.privacy strong {
		display: block;
		color: var(--text);
		font-weight: 600;
		margin-bottom: 2px;
	}
	.group {
		margin: 16px 0 0 0;
		padding: 16px 0 0 16px;
		box-shadow: inset 1px 0 0 var(--line-strong);
	}
	.intro {
		color: var(--muted);
		font-size: 13px;
		margin: 0 0 12px;
		max-width: 60ch;
	}
	.presets {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		margin: 8px 0 4px;
	}
	.preset {
		height: 28px;
		padding: 0 12px;
		border: 0;
		border-radius: 12px;
		background: var(--sunken);
		color: var(--muted);
		cursor: pointer;
	}
	.preset:hover {
		color: var(--text);
	}
	.preset.on {
		background: var(--text);
		color: var(--ground);
	}
	.preset.static {
		display: inline-flex;
		align-items: center;
		cursor: default;
		opacity: 0.6;
	}
	.preset.static.on {
		opacity: 1;
	}
	.keys,
	.quick {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}
	.keys {
		margin: 0 0 8px;
	}
	.key {
		display: inline-flex;
		align-items: center;
		gap: 2px;
		height: 28px;
		padding: 0 2px 0 10px;
		border-radius: 4px;
		background: var(--raised);
		box-shadow: inset 0 -2px 0 var(--line-strong);
		font-family: var(--font-mono);
		font-size: 12px;
	}
	.key button {
		width: 22px;
		height: 22px;
		border: 0;
		background: none;
		color: var(--muted);
		cursor: pointer;
	}
	.key button:hover {
		color: var(--text);
	}
	.listen {
		height: 28px;
		padding: 0 12px;
		border: 0;
		border-radius: 4px;
		background: transparent;
		box-shadow: inset 0 0 0 1px var(--line-strong);
		color: var(--muted);
		cursor: pointer;
	}
	.listen.on {
		box-shadow: inset 0 0 0 1px var(--text);
		color: var(--text);
	}
	.quick {
		margin-bottom: 8px;
	}
	.quick button {
		height: 24px;
		padding: 0 8px;
		border: 0;
		border-radius: 4px;
		background: transparent;
		color: var(--faint);
		font-family: var(--font-mono);
		font-size: 11px;
		cursor: pointer;
	}
	.quick button:hover {
		color: var(--text);
		background: var(--raised);
	}
	select {
		height: 32px;
		padding: 0 8px;
		border: 0;
		border-radius: 4px;
		background: var(--sunken);
		outline: none;
	}
	input[type='range'] {
		width: 160px;
		accent-color: var(--text);
	}
	input[type='color'] {
		width: 32px;
		height: 32px;
		padding: 0;
		border: none;
		background: none;
	}
	.readout {
		width: 56px;
		text-align: right;
	}
</style>
