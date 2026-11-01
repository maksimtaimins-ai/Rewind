<script lang="ts">
	import { fileUrl, fmtDuration, fmtWhen, type Clip } from '#lib/api.ts';
	import { inApp } from '#lib/stores/app.svelte.ts';

	let {
		clip,
		selected = false,
		selecting = false,
		onopen,
		onselect,
		onfavorite
	}: {
		clip: Clip;
		selected?: boolean;
		selecting?: boolean;
		onopen: () => void;
		onselect: () => void;
		onfavorite?: (favorite: boolean) => void;
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
	{#if onfavorite && !selecting}
		<button
			class="star"
			class:on={clip.favorite}
			aria-pressed={clip.favorite}
			aria-label={clip.favorite ? `Remove ${clip.name} from favorites` : `Add ${clip.name} to favorites`}
			onclick={() => onfavorite(!clip.favorite)}
		>
			<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z" /></svg>
		</button>
	{/if}
	<div class="meta">
		<div class="name" title={clip.name}>{clip.name}</div>
		<div class="when">{fmtWhen(clip.modified)}</div>
	</div>
</article>

<style>
	.tile {
		position: relative;
		min-width: 0;
	}
	.star {
		position: absolute;
		top: 6px;
		right: 6px;
		width: 28px;
		height: 28px;
		display: grid;
		place-items: center;
		border: 0;
		border-radius: 4px;
		background: var(--scrim-soft);
		color: var(--on-scrim);
		cursor: pointer;
		opacity: 0;
	}
	.tile:hover .star,
	.star:focus-visible,
	.star.on {
		opacity: 1;
	}
	.star svg {
		width: 16px;
		height: 16px;
		fill: none;
		stroke: currentColor;
		stroke-width: 1.6;
		stroke-linejoin: round;
	}
	.star.on svg {
		fill: var(--accent);
		stroke: var(--accent);
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
		outline: 2px solid var(--accent);
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
		background: var(--scrim);
		color: var(--on-scrim);
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
		border: 2px solid var(--on-scrim);
		background: var(--scrim-soft);
		color: var(--on-scrim);
		font-size: 12px;
	}
	.selected .check {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--on-accent);
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
