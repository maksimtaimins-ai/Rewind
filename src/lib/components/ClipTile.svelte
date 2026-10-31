<script lang="ts">
	import { fileUrl, fmtDuration, fmtWhen, type Clip } from '#lib/api.ts';
	import { inApp } from '#lib/stores/app.svelte.ts';

	let {
		clip,
		selected = false,
		selecting = false,
		onopen,
		onselect
	}: {
		clip: Clip;
		selected?: boolean;
		selecting?: boolean;
		onopen: () => void;
		onselect: () => void;
	} = $props();

	const thumb = $derived(clip.thumb && inApp ? fileUrl(clip.thumb) : null);
</script>

<article class="tile" class:selected>
	<button
		class="frame"
		onclick={(e) => (selecting || e.ctrlKey || e.shiftKey ? onselect() : onopen())}
		aria-label="{selecting ? 'Select' : 'Open'} {clip.name}"
	>
		{#if thumb}
			<img src={thumb} alt="" loading="lazy" />
		{:else}
			<div class="blank"></div>
		{/if}
		<span class="len num">{fmtDuration(clip.duration)}</span>
		{#if selecting}
			<span class="check" aria-hidden="true">{selected ? '✓' : ''}</span>
		{/if}
	</button>
	<div class="meta">
		<div class="name" title={clip.name}>{clip.name}</div>
		<div class="when">{fmtWhen(clip.modified)}</div>
	</div>
</article>

<style>
	.tile {
		min-width: 0;
	}
	.frame {
		position: relative;
		display: block;
		width: 100%;
		aspect-ratio: 16 / 9;
		padding: 0;
		border: 0;
		border-radius: 4px;
		overflow: hidden;
		background: var(--sunken);
		cursor: pointer;
	}
	.selected .frame {
		outline: 2px solid var(--text);
		outline-offset: 2px;
	}
	img,
	.blank {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}
	.blank {
		background: var(--sunken);
	}
	.len {
		position: absolute;
		right: 8px;
		bottom: 8px;
		padding: 2px 4px;
		border-radius: 4px;
		background: rgb(0 0 0 / 0.72);
		color: #fff;
		font-size: 11px;
	}
	.check {
		position: absolute;
		left: 8px;
		top: 8px;
		width: 20px;
		height: 20px;
		display: grid;
		place-items: center;
		border-radius: 4px;
		border: 2px solid #fff;
		background: rgb(0 0 0 / 0.4);
		color: #fff;
		font-size: 12px;
	}
	.selected .check {
		background: #fff;
		color: #000;
	}
	.meta {
		padding: 8px 0 0;
	}
	.name {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		font-weight: 500;
	}
	.when {
		color: var(--muted);
		font-size: 12px;
	}
</style>
