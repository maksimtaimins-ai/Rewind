<script lang="ts">
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import { open } from '@tauri-apps/plugin-dialog';
	import { api, fileUrl, fmtDuration, type Clip } from '#lib/api.ts';
	import { app, inApp, refreshClips, toast } from '#lib/stores/app.svelte.ts';
	import { draft, timeline, totalLength, MAX_TOTAL } from '#lib/montage.svelte.ts';
	import Segmented from '#lib/components/Segmented.svelte';
	import Toggle from '#lib/components/Toggle.svelte';
	import EmptyState from '#lib/components/EmptyState.svelte';

	type Order = 'manual' | 'newest' | 'oldest';
	let picked = $state<string[]>(draft.paths.filter((p) => app.clips.some((c) => c.path === p)));
	let order = $state<Order>('manual');
	let crossfade = $state(true);
	let max = $state<number | null>(10);
	let music = $state<string | null>(null);
	let clipAudio = $state(true);
	let progress = $state<number | null>(null);

	const byPath = (p: string) => app.clips.find((c) => c.path === p);
	const ordered = $derived.by(() => {
		const list = picked.map(byPath).filter((c): c is Clip => !!c);
		if (order === 'newest') return [...list].sort((a, b) => b.modified - a.modified);
		if (order === 'oldest') return [...list].sort((a, b) => a.modified - b.modified);
		return list;
	});
	const pieces = $derived(timeline(ordered, max, crossfade));
	const total = $derived(totalLength(pieces));
	// Montages are made from replay clips: no screenshots, no earlier montages.
	const pool = $derived(app.clips.filter((c) => !c.name.startsWith('Montage ')));

	const thumb = (c: Clip) => (c.thumb && inApp ? `${fileUrl(c.thumb)}?v=${c.thumb_rev}` : null);
	const toggle = (c: Clip) => (picked = picked.includes(c.path) ? picked.filter((p) => p !== c.path) : [...picked, c.path]);

	// Manual order: drag a piece of the filmstrip onto another, or Alt+arrows.
	let dragging = $state<string | null>(null);
	function moveTo(path: string, target: string) {
		if (path === target) return;
		const list = ordered.map((c) => c.path);
		const from = list.indexOf(path);
		list.splice(from, 1);
		list.splice(list.indexOf(target) + (from <= list.indexOf(target) ? 1 : 0), 0, path);
		picked = list;
		order = 'manual';
	}
	function nudge(path: string, by: number) {
		const list = ordered.map((c) => c.path);
		const i = list.indexOf(path);
		const j = i + by;
		if (j < 0 || j >= list.length) return;
		[list[i], list[j]] = [list[j], list[i]];
		picked = list;
		order = 'manual';
	}

	const isAudio = (p: string) => /\.(mp3|wav)$/i.test(p);
	async function chooseMusic() {
		if (!inApp) return;
		const f = await open({ filters: [{ name: 'Music', extensions: ['mp3', 'wav'] }], title: 'Music for the montage' });
		if (typeof f === 'string') music = f;
	}
	let over = $state(false);
	onMount(() => {
		if (!inApp) return;
		const offs: Promise<() => void>[] = [listen<number>('montage-progress', (e) => progress !== null && (progress = e.payload))];
		offs.push(
			import('@tauri-apps/api/webview').then(({ getCurrentWebview }) =>
				getCurrentWebview().onDragDropEvent((e) => {
					if (e.payload.type === 'over') over = true;
					else if (e.payload.type === 'drop') {
						over = false;
						const f = e.payload.paths.find(isAudio);
						if (f) music = f;
						else if (e.payload.paths.length) toast('warning', 'Drop an mp3 or wav file for music.');
					} else over = false;
				})
			)
		);
		return () => offs.forEach((o) => o.then((f) => f()));
	});

	async function make() {
		if (!pieces.length || progress !== null) return;
		if (!inApp) {
			toast('info', 'Montages are made inside the app.');
			return;
		}
		progress = 0;
		try {
			const out = await api.makeMontage({ paths: ordered.map((c) => c.path), transition: crossfade ? 'crossfade' : 'cut', max_secs: max, music, clip_audio: clipAudio });
			await refreshClips();
			draft.paths = [];
			toast('success', `Made "${out.name}"`);
			goto(`/trim?path=${encodeURIComponent(out.path)}`);
		} catch (e) {
			toast('error', `Montage failed: ${e}`);
		} finally {
			progress = null;
		}
	}
</script>

<div class="page">
	<nav class="crumbs"><a href="/library">Clips</a></nav>
	<header>
		<h1>Make a montage</h1>
		<p class="sum">
			{#if pieces.length}
				<span class="num">{pieces.length}</span> {pieces.length === 1 ? 'clip' : 'clips'}, <span class="num">{fmtDuration(total)}</span> long{#if total >= MAX_TOTAL - 0.01}, capped at 10 minutes{/if}
			{:else}
				Pick clips below.
			{/if}
		</p>
	</header>

	<section class="strip" aria-label="Montage timeline" class:empty={!pieces.length}>
		{#if pieces.length}
			<div class="film">
				{#each pieces as p, i (p.clip.path)}
					{@const t = thumb(p.clip)}
					<div
						class="piece"
						class:dragging={dragging === p.clip.path}
						style="left: {(p.outStart / total) * 100}%; width: {(p.len / total) * 100}%; z-index: {i + 1}; --fade: {(p.fadeIn / p.len) * 100}%"
						draggable="true"
						ondragstart={(e) => ((dragging = p.clip.path), e.dataTransfer?.setData('text/plain', p.clip.path))}
						ondragend={() => (dragging = null)}
						ondragover={(e) => e.preventDefault()}
						ondrop={(e) => (e.preventDefault(), dragging && moveTo(dragging, p.clip.path), (dragging = null))}
						onkeydown={(e) => e.altKey && (e.key === 'ArrowLeft' ? nudge(p.clip.path, -1) : e.key === 'ArrowRight' && nudge(p.clip.path, 1))}
						tabindex="0"
						role="button"
						aria-roledescription="movable clip"
						aria-label="{p.clip.name}, {p.len.toFixed(1)} seconds. Alt+arrows to move."
						title={p.clip.name}
					>
						{#if t}<img src={t} alt="" />{:else}<span class="blank" style:--hue={(i * 67) % 360}></span>{/if}
						<span class="label"><span class="num">{p.len.toFixed(1)}s</span></span>
					</div>
				{/each}
			</div>
			<div class="ruler num" aria-hidden="true">
				{#each Array.from({ length: Math.min(12, Math.floor(total / Math.max(1, Math.ceil(total / 10))) + 1) }, (_, i) => i * Math.max(1, Math.ceil(total / 10))) as s (s)}
					<span style="left: {(s / total) * 100}%">{fmtDuration(s)}</span>
				{/each}
			</div>
			<p class="hint">{order === 'manual' ? 'Drag pieces to reorder. ' : ''}{max ? `Each clip gives up to ${max}s, centred on its best highlight or ending where you saved it.` : 'Whole clips.'}</p>
		{:else}
			<p class="placeholder">Your montage appears here as you pick clips, in the order it will play.</p>
		{/if}
	</section>

	<div class="body">
		<section class="pick" aria-label="Clips">
			{#if pool.length}
				<div class="grid">
					{#each pool as c (c.path)}
						{@const on = picked.includes(c.path)}
						{@const t = thumb(c)}
						<button class="tile" class:on aria-pressed={on} onclick={() => toggle(c)}>
							<span class="img">
								{#if t}<img src={t} alt="" loading="lazy" />{/if}
								<span class="len num">{fmtDuration(c.duration)}</span>
								<span class="check" aria-hidden="true">{#if on}<span class="num">{ordered.findIndex((x) => x.path === c.path) + 1}</span>{/if}</span>
								{#if c.highlights?.length}<span class="hl" title="Has highlights" aria-label="Has highlights"></span>{/if}
							</span>
							<span class="name">{c.name}</span>
						</button>
					{/each}
				</div>
			{:else}
				<EmptyState>No clips yet. Save a few, then come back to string them together.</EmptyState>
			{/if}
		</section>

		<aside class="opts" aria-label="Montage options">
			<div class="opt">
				<span class="lbl">Order</span>
				<Segmented label="Order" value={order} options={[{ value: 'manual' as Order, label: 'As picked' }, { value: 'newest' as Order, label: 'Newest' }, { value: 'oldest' as Order, label: 'Oldest' }]} onchange={(v) => (order = v)} />
			</div>
			<div class="opt">
				<span class="lbl">Between clips</span>
				<Segmented label="Transition" value={crossfade ? 'x' : 'cut'} options={[{ value: 'cut', label: 'Hard cut' }, { value: 'x', label: '0.5s crossfade' }]} onchange={(v) => (crossfade = v === 'x')} />
			</div>
			<div class="opt">
				<span class="lbl">From each clip</span>
				<Segmented label="Per-clip length" value={max ?? 0} options={[5, 10, 15, 30].map((v) => ({ value: v, label: `${v}s` })).concat([{ value: 0, label: 'All' }])} onchange={(v) => (max = v || null)} />
			</div>
			<div class="opt">
				<span class="lbl">Music</span>
				<div class="drop" class:over class:set={!!music}>
					{#if music}
						<span class="file" title={music}>{music.split(/[\\/]/).pop()}</span>
						<button class="link" onclick={() => (music = null)}>Remove</button>
					{:else}
						<span>Drop an mp3 or wav here, or</span>
						<button class="link" onclick={chooseMusic}>choose a file</button>
					{/if}
				</div>
			</div>
			<div class="opt row">
				<span class="lbl">Keep the clips' sound{music ? ', quieter under the music' : ''}</span>
				<Toggle checked={clipAudio} label="Keep clip sound" onchange={(v) => (clipAudio = v)} />
			</div>
			<button class="primary" disabled={!pieces.length || progress !== null} onclick={make}>
				{#if progress !== null}
					<span class="bar" style:width="{Math.round(progress * 100)}%"></span>
					<span class="txt">Making… <span class="num">{Math.round(progress * 100)}%</span></span>
				{:else}
					<span class="txt">Make montage</span>
				{/if}
			</button>
			<p class="note">Saved to your clips folder as a new clip. Nothing is uploaded.</p>
		</aside>
	</div>
</div>

<style>
	.page {
		padding: 16px 32px 32px;
		display: grid;
		gap: 20px;
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
		align-items: baseline;
		justify-content: space-between;
		gap: 16px;
	}
	h1 {
		margin: 0;
		font-size: 24px;
		font-weight: 650;
		letter-spacing: -0.02em;
	}
	.sum {
		margin: 0;
		color: var(--muted);
	}
	/* The filmstrip: the montage as it will play, to scale. */
	.strip {
		padding: 14px 14px 10px;
		border-radius: 8px;
		background: var(--sunken);
	}
	.film {
		position: relative;
		height: 84px;
	}
	.piece {
		position: absolute;
		top: 0;
		bottom: 0;
		overflow: hidden;
		border-radius: 4px;
		background: var(--media);
		cursor: grab;
		/* A crossfade shows as the piece fading in over the one before it. */
		mask-image: linear-gradient(90deg, transparent 0, #000 var(--fade));
		box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.06);
		transition: left 220ms var(--ease), width 220ms var(--ease);
	}
	.piece:focus-visible {
		outline: 2px solid var(--text);
		outline-offset: 2px;
	}
	.piece.dragging {
		opacity: 0.4;
	}
	.piece img,
	.blank {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
	.blank {
		background: linear-gradient(135deg, hsl(var(--hue) 30% 26%), hsl(calc(var(--hue) + 40) 30% 14%));
	}
	.label {
		position: absolute;
		left: 6px;
		bottom: 5px;
		padding: 1px 5px;
		border-radius: 4px;
		background: var(--scrim);
		color: var(--on-scrim);
		font-size: 10.5px;
	}
	.ruler {
		position: relative;
		height: 16px;
		margin-top: 6px;
		color: var(--faint);
		font-size: 10px;
	}
	.ruler span {
		position: absolute;
		top: 0;
		padding-left: 4px;
		box-shadow: inset 1px 0 0 var(--line-strong);
	}
	.hint,
	.placeholder {
		margin: 4px 0 0;
		color: var(--faint);
		font-size: 12px;
	}
	.strip.empty {
		display: grid;
		place-items: center;
		min-height: 84px;
		box-shadow: inset 0 0 0 1px var(--line);
		background: transparent;
	}
	.placeholder {
		margin: 0;
		color: var(--muted);
		font-size: 13px;
	}
	.body {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 300px;
		gap: 32px;
		align-items: start;
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(176px, 1fr));
		gap: 16px;
	}
	.tile {
		display: grid;
		gap: 6px;
		padding: 0;
		border: 0;
		background: none;
		text-align: left;
		cursor: pointer;
	}
	.img {
		position: relative;
		aspect-ratio: 16 / 9;
		border-radius: 4px;
		overflow: hidden;
		background: var(--raised);
		box-shadow: inset 0 0 0 1px var(--line);
		transition: box-shadow 140ms var(--ease);
	}
	.img img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}
	.tile.on .img {
		box-shadow: 0 0 0 2px var(--text);
	}
	.len {
		position: absolute;
		right: 6px;
		bottom: 6px;
		padding: 1px 5px;
		border-radius: 4px;
		background: var(--scrim);
		color: var(--on-scrim);
		font-size: 10.5px;
	}
	.check {
		position: absolute;
		left: 6px;
		top: 6px;
		width: 20px;
		height: 20px;
		display: grid;
		place-items: center;
		border-radius: 50%;
		background: var(--scrim);
		box-shadow: inset 0 0 0 1.5px var(--on-scrim);
		color: var(--ground);
		font-size: 10.5px;
	}
	.tile.on .check {
		background: var(--text);
		box-shadow: none;
	}
	.hl {
		position: absolute;
		right: 8px;
		top: 8px;
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--mark);
	}
	.name {
		color: var(--muted);
		font-size: 12px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.tile.on .name {
		color: var(--text);
	}
	.opts {
		position: sticky;
		top: 16px;
		display: grid;
		gap: 18px;
	}
	.opt {
		display: grid;
		gap: 8px;
	}
	.opt.row {
		grid-template-columns: 1fr auto;
		align-items: center;
	}
	.lbl {
		font-weight: 500;
	}
	.drop {
		padding: 14px;
		border-radius: 4px;
		box-shadow: inset 0 0 0 1px var(--line-strong);
		color: var(--muted);
		font-size: 13px;
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
		align-items: center;
		transition: box-shadow 140ms var(--ease);
	}
	.drop.over {
		box-shadow: inset 0 0 0 2px var(--text);
	}
	.drop.set {
		justify-content: space-between;
		color: var(--text);
	}
	.file {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.link {
		border: 0;
		background: none;
		padding: 0;
		color: var(--text);
		text-decoration: underline;
		cursor: pointer;
	}
	.primary {
		position: relative;
		overflow: hidden;
		height: 40px;
		border: 0;
		border-radius: 4px;
		background: var(--accent);
		color: var(--on-accent);
		font-weight: 600;
		cursor: pointer;
	}
	.primary:disabled {
		cursor: default;
	}
	.primary:disabled:not(:has(.bar)) {
		background: var(--line);
		color: var(--faint);
	}
	.bar {
		position: absolute;
		left: 0;
		top: 0;
		bottom: 0;
		background: rgb(255 255 255 / 0.18);
		transition: width 200ms linear;
	}
	.txt {
		position: relative;
	}
	.note {
		margin: -8px 0 0;
		color: var(--faint);
		font-size: 12px;
	}
	@media (max-width: 860px) {
		.body {
			grid-template-columns: 1fr;
		}
		.opts {
			position: static;
		}
	}
</style>
