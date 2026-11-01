<!-- Settings > Watch folders: input -> preset -> output pairs, plus recent history. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import { open } from '@tauri-apps/plugin-dialog';
	import { api, fmtWhen, type ExportPreset, type Settings, type WatchOp } from '#lib/api.ts';
	import { app, inApp, updateSettings } from '#lib/stores/app.svelte.ts';
	import Toggle from './Toggle.svelte';

	const pairs = $derived(app.settings?.watch_folders ?? []);
	let history = $state<WatchOp[]>([]);

	const presets: { id: ExportPreset; name: string }[] = [
		{ id: 'discord10', name: 'Discord, 10 MB' },
		{ id: 'discord25', name: 'Discord, 25 MB' },
		{ id: 'shorts', name: 'YouTube Shorts' },
		{ id: 'twitter', name: 'X / Twitter' },
		{ id: 'gif', name: 'GIF' }
	];

	onMount(() => {
		if (!inApp) {
			history = [
				{ at_ms: Date.now() - 6e5, input: 'C:\\clips\\outbox\\shorts\\Ace.mp4', output: 'C:\\clips\\exported\\shorts\\Ace-shorts.mp4', preset: 'shorts', error: null, deleted: false }
			];
			return;
		}
		api.watchHistory().then((h) => (history = h));
		const off = listen<WatchOp[]>('watch-history', (e) => (history = e.payload));
		return () => void off.then((f) => f());
	});

	const set = (f: (x: Settings) => void) => updateSettings(f);
	const base = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;

	async function pick(title: string, current?: string) {
		if (!inApp) return null;
		const dir = await open({ directory: true, defaultPath: current || app.settings?.clips_dir, title });
		return typeof dir === 'string' ? dir : null;
	}

	async function add() {
		const input = await pick('Watch this folder for new clips');
		if (!input) return;
		const output = await pick('Put exported copies in');
		if (!output) return;
		set((x) => x.watch_folders.push({ enabled: true, input, preset: 'shorts', output, delete_original: false }));
	}

	async function change(i: number, which: 'input' | 'output') {
		const dir = await pick(which === 'input' ? 'Watch this folder for new clips' : 'Put exported copies in', pairs[i][which]);
		if (dir) set((x) => (x.watch_folders[i][which] = dir));
	}
</script>

<p class="intro">Drop an .mp4 into an input folder and Rewind exports it with that folder's preset into the output folder. Files still being copied are left until they stop changing.</p>

{#each pairs as w, i (i)}
	{@const same = w.input.toLowerCase() === w.output.toLowerCase()}
	<div class="pair" class:off={!w.enabled}>
		<div class="flow">
			<button class="path" onclick={() => change(i, 'input')} title={w.input}><span class="k">From</span>{base(w.input)}</button>
			<select value={w.preset} onchange={(e) => set((x) => (x.watch_folders[i].preset = (e.currentTarget as HTMLSelectElement).value as ExportPreset))} aria-label="Preset for {base(w.input)}">
				{#each presets as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
			</select>
			<button class="path" onclick={() => change(i, 'output')} title={w.output}><span class="k">To</span>{base(w.output)}</button>
			<Toggle checked={w.enabled} label="Watch {base(w.input)}" onchange={(v) => set((x) => (x.watch_folders[i].enabled = v))} />
		</div>
		<div class="opts">
			<label><input type="checkbox" checked={w.delete_original} onchange={(e) => set((x) => (x.watch_folders[i].delete_original = (e.currentTarget as HTMLInputElement).checked))} /> Delete original after export</label>
			<button class="link" onclick={() => set((x) => x.watch_folders.splice(i, 1))}>Remove</button>
		</div>
		{#if same}<p class="err">Pick a different output folder. Exports would land in the watched folder.</p>{/if}
	</div>
{/each}
<div class="add">
	<button class="secondary" disabled={!inApp} onclick={add}>Add watch folder</button>
</div>

{#if history.length}
	<h3>History</h3>
	<ul class="history">
		{#each history as op, i (i)}
			<li>
				<span class="when">{fmtWhen(op.at_ms)}</span>
				<span class="what" title={op.input}>{base(op.input)}</span>
				{#if op.error}
					<span class="err">{op.error}</span>
				{:else}
					<span class="out" title={op.output}>→ {base(op.output ?? '')}{op.deleted ? ', original deleted' : ''}</span>
				{/if}
			</li>
		{/each}
	</ul>
{/if}

<style>
	.intro {
		color: var(--muted);
		font-size: 13px;
		margin: 0 0 8px;
		max-width: 60ch;
	}
	.pair {
		padding: 12px 0;
		border-bottom: 1px solid var(--line);
	}
	.pair.off .flow > :not(:last-child) {
		opacity: 0.5;
	}
	.flow {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr) auto;
		gap: 8px;
		align-items: center;
	}
	.path {
		display: flex;
		gap: 8px;
		min-width: 0;
		height: 32px;
		padding: 0 8px;
		align-items: center;
		border: 0;
		border-radius: 4px;
		background: var(--sunken);
		text-align: left;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		cursor: pointer;
	}
	.k {
		color: var(--faint);
		font-size: 12px;
	}
	select {
		height: 32px;
		padding: 0 8px;
		border: 0;
		border-radius: 4px;
		background: var(--sunken);
	}
	.opts {
		display: flex;
		justify-content: space-between;
		margin-top: 8px;
		color: var(--muted);
		font-size: 13px;
	}
	.opts label {
		display: flex;
		gap: 6px;
		align-items: center;
	}
	.err {
		color: var(--danger, var(--signal));
		font-size: 13px;
		margin: 4px 0 0;
	}
	.add {
		margin-top: 12px;
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
	.link {
		border: 0;
		background: none;
		color: var(--muted);
		cursor: pointer;
	}
	.link:hover {
		color: var(--text);
	}
	h3 {
		margin: 24px 0 4px;
		font-size: 13px;
		font-weight: 600;
		color: var(--muted);
	}
	.history {
		list-style: none;
		margin: 0;
		padding: 0;
		font-size: 13px;
	}
	.history li {
		display: grid;
		grid-template-columns: 120px minmax(0, 1fr) minmax(0, 1.4fr);
		gap: 12px;
		padding: 4px 0;
	}
	.when {
		color: var(--faint);
	}
	.what,
	.out {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.out {
		color: var(--muted);
	}
</style>
