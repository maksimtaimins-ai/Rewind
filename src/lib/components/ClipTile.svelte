<script lang="ts" module>
	// Only a fresh "favorite" click celebrates, not tiles that load already starred.
	// Module-wide: starring can move the clip into the Favorites grid, a new tile.
	let starred = $state<string | null>(null);
</script>

<script lang="ts">
	import { fileUrl, fmtDuration, fmtWhen, type Clip } from '#lib/api.ts';
	import { inApp } from '#lib/stores/app.svelte.ts';
	import TagChip from './TagChip.svelte';
	import { inview } from '#lib/inview.ts';

	let {
		clip,
		selected = false,
		selecting = false,
		onopen,
		onselect,
		onfavorite,
		ondelete,
		onedittags,
		ontag,
		activeTags = [],
		oncontextmenu
	}: {
		clip: Clip;
		selected?: boolean;
		selecting?: boolean;
		onopen: () => void;
		onselect: () => void;
		onfavorite?: (favorite: boolean) => void;
		ondelete?: () => void;
		/** Open the tag editor, anchored at the clicked button. */
		onedittags?: (e: MouseEvent) => void;
		/** Click on one of the clip's tags. */
		ontag?: (tag: string) => void;
		activeTags?: string[];
		oncontextmenu?: (e: MouseEvent) => void;
	} = $props();

	// The revision busts the webview's image cache when a thumbnail is replaced.
	const thumb = $derived(clip.thumb && inApp ? `${fileUrl(clip.thumb)}?v=${clip.thumb_rev}` : null);
	// Thumbnails are fetched and decoded only once the tile is near the viewport.
	let near = $state(false);
	const star = 'M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z';
</script>

<article class="tile" class:selected oncontextmenu={oncontextmenu && ((e) => (e.preventDefault(), oncontextmenu(e)))}>
	<button
		class="frame"
		onclick={(e) => (selecting || e.ctrlKey || e.shiftKey ? onselect() : onopen())}
		aria-label="{selecting ? 'Select' : 'Open'} {clip.name}"
	>
		<div class="art" use:inview={{ onenter: () => (near = true) }}>
			{#if thumb && near}
				<img src={thumb} alt="" decoding="async" />
			{:else}
				<div class="blank"></div>
			{/if}
		</div>
		<!-- Name and length sit on a scrim at the bottom of the thumbnail. -->
		<span class="shade">
			<span class="name" title={clip.name}>{clip.name}</span>
			<span class="sub"><span class="num">{fmtDuration(clip.duration)}</span><span>{fmtWhen(clip.modified)}</span></span>
		</span>
		{#if selecting}
			<span class="check" aria-hidden="true">
				{#if selected}<svg viewBox="0 0 24 24"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>{/if}
			</span>
		{:else if clip.favorite}
			<!-- State, not an action: stays visible. -->
			<span class="fav-mark" aria-hidden="true"><svg viewBox="0 0 24 24"><path d={star} /></svg></span>
		{/if}
	</button>
	{#if !selecting}
		<div class="actions">
			<button class="act" aria-label="Play {clip.name}" title="Play" onclick={onopen}>
				<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5.5v13l10.5-6.5z" /></svg>
			</button>
			{#if onfavorite}
				<button
					class="act"
					class:on={clip.favorite}
					class:pop={starred === clip.path && clip.favorite}
					onanimationend={() => (starred = null)}
					aria-pressed={clip.favorite}
					aria-label={clip.favorite ? `Remove ${clip.name} from favorites` : `Add ${clip.name} to favorites`}
					title={clip.favorite ? 'Unfavorite' : 'Favorite'}
					onclick={() => {
						starred = clip.favorite ? null : clip.path;
						onfavorite(!clip.favorite);
					}}
				>
					<svg viewBox="0 0 24 24" aria-hidden="true"><path d={star} /></svg>
				</button>
			{/if}
			{#if onedittags}
				<button class="act" aria-label="Tag {clip.name}" title="Tag" onclick={onedittags}>
					<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 4h8l8 8-8 8-8-8z" /><circle cx="8.5" cy="8.5" r="1.2" /></svg>
				</button>
			{/if}
			{#if ondelete}
				<button class="act" aria-label="Delete {clip.name}" title="Delete" onclick={ondelete}>
					<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 7h14M10 7V4.5h4V7M7 7l1 13h8l1-13" /></svg>
				</button>
			{/if}
		</div>
	{/if}
	{#if clip.tags.length && !selecting}
		<div class="tags" style:left={clip.favorite ? '32px' : '8px'}>
			{#each clip.tags.slice(0, 3) as t (t)}
				<TagChip tag={t} active={activeTags.includes(t)} onclick={ontag && (() => ontag(t))} />
			{/each}
			{#if clip.tags.length > 3}<span class="more num">+{clip.tags.length - 3}</span>{/if}
		</div>
	{/if}
</article>

<style>
	.tile {
		position: relative;
		min-width: 0;
		transition: transform 180ms var(--ease);
	}
	.tile:hover,
	.tile:focus-within {
		transform: scale(1.02);
		z-index: 1;
	}
	/* The glow sits on the frame so it follows its rounded corners. */
	.frame {
		transition: box-shadow 180ms var(--ease);
	}
	.tile:hover .frame,
	.tile:focus-within .frame {
		box-shadow:
			0 0 0 1px color-mix(in srgb, var(--accent) 45%, transparent),
			0 6px 22px color-mix(in srgb, var(--accent) 16%, transparent);
	}
	@media (prefers-reduced-motion: reduce) {
		.tile:hover,
		.tile:focus-within {
			transform: none;
		}
	}
	.frame {
		position: relative;
		display: block;
		width: 100%;
		aspect-ratio: 16 / 9;
		padding: 0;
		border: 0;
		border-radius: var(--r-2);
		overflow: hidden;
		background: var(--sunken);
		cursor: pointer;
	}
	.selected .frame {
		outline: 2px solid var(--text);
		outline-offset: 2px;
	}
	.art {
		width: 100%;
		height: 100%;
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
	/* Scrim, so the name reads on any frame. */
	.shade {
		position: absolute;
		inset: auto 0 0 0;
		display: grid;
		gap: 2px;
		padding: 40px var(--s-3) var(--s-3);
		text-align: left;
		background: linear-gradient(to top, rgb(0 0 0 / 0.82), rgb(0 0 0 / 0.5) 55%, transparent);
		color: var(--on-scrim);
		pointer-events: none;
	}
	.shade .name {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		font-weight: 600;
		font-size: 14px;
	}
	.shade .sub {
		display: flex;
		gap: var(--s-2);
		font-size: 12px;
		opacity: 0.78;
	}
	.frame img {
		transition: transform 400ms var(--ease);
	}
	.tile:hover .frame img {
		transform: scale(1.04);
	}
	.fav-mark {
		position: absolute;
		left: 8px;
		top: 8px;
		width: 16px;
		height: 16px;
		color: var(--on-scrim);
		filter: drop-shadow(0 0 2px var(--scrim));
	}
	.fav-mark svg {
		width: 100%;
		height: 100%;
		fill: currentColor;
	}
	.check {
		position: absolute;
		left: 8px;
		top: 8px;
		width: 20px;
		height: 20px;
		display: grid;
		place-items: center;
		box-sizing: border-box;
		border-radius: 4px;
		border: 2px solid var(--on-scrim);
		background: var(--scrim-soft);
		color: var(--on-scrim);
	}
	.check svg {
		width: 14px;
		height: 14px;
		fill: none;
		stroke: currentColor;
		stroke-width: 2.4;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
	.selected .check {
		background: var(--text);
		border-color: var(--text);
		color: var(--ground);
	}

	/* Actions surface on hover (or keyboard focus), never permanently. */
	.actions {
		position: absolute;
		top: 8px;
		right: 8px;
		display: flex;
		gap: 4px;
		opacity: 0;
		pointer-events: none;
		transition: opacity 120ms ease-out;
	}
	.tile:hover .actions,
	.actions:focus-within {
		opacity: 1;
		pointer-events: auto;
	}
	.act {
		width: 28px;
		height: 28px;
		display: grid;
		place-items: center;
		border: 0;
		border-radius: 4px;
		background: var(--scrim);
		color: var(--on-scrim);
		cursor: pointer;
	}
	.act:hover {
		background: var(--media);
	}
	.act svg {
		width: 16px;
		height: 16px;
		fill: none;
		stroke: currentColor;
		stroke-width: 1.6;
		stroke-linejoin: round;
		stroke-linecap: round;
	}
	.act:first-child svg,
	.act.on svg {
		fill: currentColor;
	}
	.act.pop svg {
		animation: star-pop 300ms var(--ease);
	}
	@keyframes star-pop {
		0% {
			transform: scale(0.6);
		}
		45% {
			transform: scale(1.3);
			filter: drop-shadow(0 0 6px currentColor);
		}
		100% {
			transform: scale(1);
			filter: drop-shadow(0 0 0 transparent);
		}
	}
	/* Tags float over the top-left of the thumbnail, out of the way of the name. */
	.tags {
		position: absolute;
		left: 8px;
		top: 8px;
		display: flex;
		gap: 4px;
		max-width: calc(100% - 140px);
		overflow: hidden;
	}
	.more {
		align-self: center;
		padding: 0 4px;
		color: var(--on-scrim);
		font-size: 11px;
	}
</style>
