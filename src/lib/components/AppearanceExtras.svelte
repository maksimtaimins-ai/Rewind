<!--
  Settings → Appearance: accent colour, typeface, UI scale and the dashboard's
  status pills. Applied by the layout (appearance.ts, themes.ts).
-->
<script lang="ts">
	import { fmtBytes, fmtDate, fmtDuration, type Settings } from '#lib/api.ts';
	import { app, updateSettings } from '#lib/stores/app.svelte.ts';
	import Row from './Row.svelte';
	import Segmented from './Segmented.svelte';
	import Toggle from './Toggle.svelte';

	type Look = Settings['appearance'];
	const look = $derived(app.settings!.appearance);
	const set = (f: (a: Look) => void) => updateSettings((x) => f(x.appearance));

	const swatches = ['#4f8cff', '#8b5cf6', '#ec4899', '#ef4444', '#f59e0b', '#22c55e', '#14b8a6'];
	// The picker shows something sensible while "theme default" is selected.
	const shown = $derived(look.accent || '#4f8cff');
	let picker = $state<HTMLInputElement>();
</script>

<Row id="s-accent" label="Accent colour" hint={look.accent ? 'Your colour is used for primary buttons and highlights in every theme.' : "Using the theme's own accent."}>
	<div class="swatches" role="group" aria-label="Accent colour">
		<button class="sw none" class:on={!look.accent} aria-pressed={!look.accent} title="Theme default" aria-label="Theme default accent" onclick={() => set((a) => (a.accent = ''))}>
			<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 13L13 3" /></svg>
		</button>
		{#each swatches as c (c)}
			<button class="sw" class:on={look.accent === c} aria-pressed={look.accent === c} style:background={c} title={c} aria-label="Accent {c}" onclick={() => set((a) => (a.accent = c))}></button>
		{/each}
		<button class="sw custom" class:on={!!look.accent && !swatches.includes(look.accent)} style:--c={shown} title="Pick any colour" aria-label="Custom accent colour" onclick={() => picker?.click()}></button>
		<input bind:this={picker} class="native" type="color" value={shown} onchange={(e) => set((a) => (a.accent = e.currentTarget.value))} aria-label="Custom accent colour picker" tabindex="-1" />
	</div>
</Row>
<Row id="s-font" label="Font" hint="The typeface for the whole app. Numbers stay in the mono face.">
	<Segmented
		label="Font family"
		value={look.font}
		options={[
			{ value: 'theme' as const, label: 'Default' },
			{ value: 'inter' as const, label: 'Inter' },
			{ value: 'mono' as const, label: 'Mono' },
			{ value: 'system' as const, label: 'System' }
		]}
		onchange={(v) => set((a) => (a.font = v))}
	/>
</Row>
<Row id="s-ui-scale" label="Interface size" hint="Scales the whole window, text and all.">
	<Segmented
		label="UI scale"
		value={look.ui_scale}
		options={[80, 90, 100, 110, 125, 150].map((v) => ({ value: v, label: `${v}%` }))}
		onchange={(v) => set((a) => (a.ui_scale = v))}
	/>
</Row>
<Row id="s-status-pills" label="Status pills on the dashboard" hint="Encoder, frame rate, current game and buffer fill, as small tags above the widgets.">
	<Toggle checked={look.status_pills} label="Status pills" onchange={(v) => set((a) => (a.status_pills = v))} />
</Row>

<h3 id="appearance-meta">Dates, times and sizes</h3>
<Row id="s-time-format" label="Time" hint="12 or 24 hour clock.">
	<Segmented
		label="Time format"
		value={look.time_format}
		options={[
			{ value: 'system' as const, label: 'System' },
			{ value: '12h' as const, label: '12-hour' },
			{ value: '24h' as const, label: '24-hour' }
		]}
		onchange={(v) => set((a) => (a.time_format = v))}
	/>
</Row>
<Row id="s-date-format" label="Date" hint="Shown on clips, sessions and search. Example: {fmtDate(Date.now() - 5 * 86_400_000)}.">
	<select value={look.date_format} onchange={(e) => set((a) => (a.date_format = e.currentTarget.value as Look['date_format']))} aria-label="Date format">
		<option value="friendly">Today, Yesterday, Nov 5</option>
		<option value="iso">2026-11-05</option>
		<option value="dmy">05/11/2026</option>
		<option value="mdy">11/05/2026</option>
		<option value="relative">5 days ago</option>
	</select>
</Row>
<Row id="s-duration-format" label="Length" hint="How a clip's length is written. Example: {fmtDuration(83)}.">
	<Segmented
		label="Duration format"
		value={look.duration_format}
		options={[
			{ value: 'clock' as const, label: '1:23' },
			{ value: 'words' as const, label: '1m 23s' },
			{ value: 'seconds' as const, label: '83s' }
		]}
		onchange={(v) => set((a) => (a.duration_format = v))}
	/>
</Row>
<Row id="s-size-format" label="File sizes" hint="Example: {fmtBytes(1_234_567_890)}.">
	<Segmented
		label="File size format"
		value={look.size_format}
		options={[
			{ value: 'si' as const, label: 'KB / MB / GB' },
			{ value: 'iec' as const, label: 'KiB / MiB / GiB' }
		]}
		onchange={(v) => set((a) => (a.size_format = v))}
	/>
</Row>
<Row id="s-language" label="Language" hint="More languages will come later.">
	<select value={look.language} aria-label="Language" onchange={() => {}}>
		<option value="en">English</option>
	</select>
</Row>

<style>
	select {
		height: 32px;
		padding: 0 8px;
		border: 0;
		border-radius: var(--r-1);
		background: var(--sunken);
		outline: none;
	}
	h3 {
		margin: 24px 0 4px;
		font-size: 13px;
		font-weight: 600;
		color: var(--muted);
	}
	.swatches {
		display: flex;
		align-items: center;
		gap: 8px;
		position: relative;
	}
	.sw {
		width: 24px;
		height: 24px;
		padding: 0;
		border: 0;
		border-radius: 50%;
		cursor: pointer;
		box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.25);
	}
	.sw.on {
		outline: 2px solid var(--text);
		outline-offset: 2px;
	}
	.sw.none {
		background: var(--sunken);
		display: grid;
		place-items: center;
		color: var(--muted);
		box-shadow: inset 0 0 0 1px var(--line-strong);
	}
	.sw.none svg {
		width: 14px;
		height: 14px;
		fill: none;
		stroke: currentColor;
		stroke-width: 1.5;
	}
	.sw.custom {
		background: conic-gradient(#ef4444, #f59e0b, #22c55e, #14b8a6, #4f8cff, #8b5cf6, #ec4899, #ef4444);
	}
	.sw.custom.on {
		background: var(--c);
	}
	.native {
		position: absolute;
		right: 0;
		bottom: 0;
		width: 1px;
		height: 1px;
		opacity: 0;
		pointer-events: none;
	}
</style>
