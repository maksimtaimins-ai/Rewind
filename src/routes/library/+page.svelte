<script lang="ts">
	import { goto } from '$app/navigation';
	import { openPath } from '@tauri-apps/plugin-opener';
	import { api, fmtBytes, type Clip } from '#lib/api.ts';
	import { app, inApp, refreshClips, toast } from '#lib/stores/app.svelte.ts';
	import ClipTile from '#lib/components/ClipTile.svelte';

	let selecting = $state(false);
	let selected = $state<string[]>([]);
	let query = $state('');
	let busy = $state(false);

	const shown = $derived(
		query.trim() ? app.clips.filter((c) => c.name.toLowerCase().includes(query.trim().toLowerCase())) : app.clips
	);
	const totalSize = $derived(app.clips.reduce((n, c) => n + c.size, 0));

	function toggle(c: Clip) {
		selecting = true;
		selected = selected.includes(c.path) ? selected.filter((p) => p !== c.path) : [...selected, c.path];
	}

	function stopSelecting() {
		selecting = false;
		selected = [];
	}

	async function remove() {
		if (!inApp || !selected.length) return;
		busy = true;
		try {
			await api.deleteClips(selected);
			toast('success', `Moved ${selected.length} ${selected.length === 1 ? 'clip' : 'clips'} to the Recycle Bin`);
			stopSelecting();
			await refreshClips();
		} catch (e) {
			toast('error', String(e));
		} finally {
			busy = false;
		}
	}

	async function merge() {
		if (!inApp) return;
		busy = true;
		try {
			// Oldest first, so the merged clip plays in the order things happened.
			const order = app.clips.filter((c) => selected.includes(c.path)).reverse().map((c) => c.path);
			const out = await api.merge(order);
			toast('success', `Merged into "${out.name}"`);
			stopSelecting();
			await refreshClips();
		} catch (e) {
			toast('error', String(e));
		} finally {
			busy = false;
		}
	}

	async function openFolder() {
		if (inApp) await openPath(await api.clipsDir());
	}
</script>

<div class="page">
	<header>
		<div>
			<h1>Clips</h1>
			<p class="sub"><span class="num">{app.clips.length}</span> clips, <span class="num">{fmtBytes(totalSize)}</span> on disk</p>
		</div>
		<div class="tools">
			{#if selecting}
				<span class="picked"><span class="num">{selected.length}</span> selected</span>
				<button class="secondary" disabled={busy || selected.length < 2 || selected.length > 5} onclick={merge} title="Join 2–5 clips end to end">
					Merge
				</button>
				<button class="secondary" disabled={busy || !selected.length} onclick={remove}>Move to Recycle Bin</button>
				<button class="secondary" onclick={stopSelecting}>Done</button>
			{:else}
				<input type="search" placeholder="Filter by name" bind:value={query} aria-label="Filter clips by name" />
				<button class="secondary" onclick={() => (selecting = true)} disabled={!app.clips.length}>Select</button>
				<button class="secondary" onclick={openFolder}>Open folder</button>
			{/if}
		</div>
	</header>

	{#if shown.length}
		<div class="grid">
			{#each shown as c (c.path)}
				<ClipTile
					clip={c}
					{selecting}
					selected={selected.includes(c.path)}
					onopen={() => goto(`/trim?path=${encodeURIComponent(c.path)}`)}
					onselect={() => toggle(c)}
				/>
			{/each}
		</div>
	{:else if app.clips.length}
		<p class="empty">No clip names contain “{query}”.</p>
	{:else}
		<div class="empty">
			<p>No clips yet.</p>
			<p>
				While a game is running, press
				{#if app.settings?.hotkeys.save_clip}<kbd>{app.settings.hotkeys.save_clip}</kbd>{:else}your save hotkey{/if}
				and the last {app.settings?.buffer.seconds} seconds land here.
			</p>
		</div>
	{/if}
</div>

<style>
	.page {
		padding: 16px 32px 32px;
	}
	header {
		display: flex;
		justify-content: space-between;
		align-items: flex-end;
		gap: 24px;
		margin-bottom: 24px;
	}
	h1 {
		margin: 0;
		font-size: 32px;
		font-weight: 650;
		letter-spacing: -0.02em;
	}
	.sub {
		margin: 4px 0 0;
		color: var(--muted);
	}
	.tools {
		display: flex;
		gap: 8px;
		align-items: center;
	}
	.picked {
		color: var(--muted);
		margin-right: 8px;
	}
	input[type='search'] {
		height: 32px;
		width: 200px;
		padding: 0 12px;
		border-radius: 4px;
		border: 1px solid var(--line);
		background: transparent;
		outline: none;
	}
	input[type='search']:focus {
		border-color: var(--muted);
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
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
		gap: 24px 16px;
	}
	.empty {
		color: var(--muted);
		max-width: 56ch;
	}
	.empty p {
		margin: 0 0 8px;
	}
	kbd {
		font-family: var(--font-mono);
		font-size: 11px;
		padding: 2px 6px;
		border-radius: 4px;
		background: var(--raised);
		color: var(--text);
	}
</style>
