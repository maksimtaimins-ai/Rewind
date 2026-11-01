<script lang="ts">
	import { onMount } from 'svelte';
	import { api, fileUrl, fmtBytes, fmtDuration, fmtWhen, type RecycledClip } from '#lib/api.ts';
	import { app, inApp, refreshClips, toast } from '#lib/stores/app.svelte.ts';

	let items = $state<RecycledClip[]>([]);
	let loaded = $state(false);
	let busy = $state(false);

	const keepDays = $derived(app.settings?.library.purge_days ?? 30);
	const daysLeft = (ms: number) => Math.max(0, Math.ceil((ms + keepDays * 86_400_000 - Date.now()) / 86_400_000));

	async function load() {
		if (!inApp) {
			items = [
				{ id: 'a.mp4', name: 'Valorant 2026-10-28 20-11-03', original: 'C:\\clips\\a.mp4', deleted_ms: Date.now() - 3 * 86_400_000, size: 52e6, duration: 30, thumb: null },
				{ id: 'b.mp4', name: 'Desktop 2026-10-20 09-40-12', original: 'C:\\clips\\b.mp4', deleted_ms: Date.now() - 26 * 86_400_000, size: 9e6, duration: 12, thumb: null }
			];
		} else {
			items = await api.recycled();
		}
		loaded = true;
	}

	onMount(load);

	async function restore(ids: string[]) {
		if (!inApp) return;
		busy = true;
		try {
			const n = await api.restoreClips(ids);
			toast('success', `Restored ${n} ${n === 1 ? 'clip' : 'clips'}`);
			await Promise.all([load(), refreshClips()]);
		} catch (e) {
			toast('error', String(e));
		} finally {
			busy = false;
		}
	}
</script>

<div class="page">
	<nav class="crumbs"><a href="/library">Clips</a></nav>
	<header>
		<div>
			<h1>Recently deleted</h1>
			<p class="sub">Deleted clips stay here for {keepDays} days, then they're removed for good.</p>
		</div>
		{#if items.length > 1}
			<button class="secondary" disabled={busy} onclick={() => restore(items.map((i) => i.id))}>Restore all</button>
		{/if}
	</header>

	{#if items.length}
		<ul>
			{#each items as it (it.id)}
				<li>
					<span class="thumb">
						{#if it.thumb && inApp}<img src={fileUrl(it.thumb)} alt="" loading="lazy" />{/if}
						<span class="len num">{fmtDuration(it.duration)}</span>
					</span>
					<span class="info">
						<span class="name" title={it.original}>{it.name}</span>
						<span class="when">Deleted {fmtWhen(it.deleted_ms)}, <span class="num">{fmtBytes(it.size)}</span></span>
					</span>
					<span class="left" class:soon={daysLeft(it.deleted_ms) <= 3}>
						{daysLeft(it.deleted_ms) === 0 ? 'Removed soon' : `${daysLeft(it.deleted_ms)} ${daysLeft(it.deleted_ms) === 1 ? 'day' : 'days'} left`}
					</span>
					<button class="secondary" disabled={busy} onclick={() => restore([it.id])}>Restore</button>
				</li>
			{/each}
		</ul>
	{:else if loaded}
		<p class="empty">Nothing here. Clips you delete wait here for {keepDays} days in case you change your mind.</p>
	{/if}
</div>

<style>
	.page {
		padding: 16px 32px 32px;
		max-width: 960px;
	}
	.crumbs a {
		color: var(--muted);
		text-decoration: none;
		font-size: 13px;
	}
	.crumbs a:hover {
		color: var(--text);
	}
	header {
		display: flex;
		justify-content: space-between;
		align-items: flex-end;
		gap: 24px;
		margin: 8px 0 24px;
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
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	li {
		display: grid;
		grid-template-columns: 128px minmax(0, 1fr) auto auto;
		align-items: center;
		gap: 16px;
		padding: 8px 0;
	}
	.thumb {
		position: relative;
		aspect-ratio: 16 / 9;
		border-radius: 4px;
		overflow: hidden;
		background: var(--sunken);
	}
	img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
		filter: grayscale(0.6);
	}
	.len {
		position: absolute;
		right: 4px;
		bottom: 4px;
		padding: 1px 4px;
		border-radius: 4px;
		background: var(--scrim);
		color: var(--on-scrim);
		font-size: 10px;
	}
	.info {
		display: grid;
		min-width: 0;
	}
	.name {
		font-weight: 500;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.when {
		color: var(--muted);
		font-size: 12px;
	}
	.left {
		color: var(--muted);
		font-size: 13px;
	}
	.left.soon {
		color: var(--signal);
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
	.empty {
		color: var(--muted);
		max-width: 56ch;
	}
</style>
