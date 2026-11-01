<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { revealItemInDir } from '@tauri-apps/plugin-opener';
	import { api, fileUrl, fmtBytes, fmtDuration, fmtWhen, type ExportPreset } from '#lib/api.ts';
	import ExportMenu from '#lib/components/ExportMenu.svelte';
	import { app, inApp, refreshClips, toast } from '#lib/stores/app.svelte.ts';

	const path = $derived(page.url.searchParams.get('path') ?? '');
	const clip = $derived(app.clips.find((c) => c.path === path));

	let video = $state<HTMLVideoElement>();
	let current = $state(0);
	let duration = $state(0);
	let paused = $state(true);
	let keyframes = $state<number[]>([]);
	let inPt = $state(0);
	let outPt = $state(0);
	let name = $state('');
	let busy = $state(false);
	let track = $state<HTMLDivElement>();
	let drag: 'in' | 'out' | 'head' | null = null;

	$effect(() => {
		name = clip?.name ?? '';
	});

	$effect(() => {
		const p = path;
		if (!p) return;
		if (!inApp) {
			keyframes = Array.from({ length: 60 }, (_, i) => i);
			duration = 60;
			outPt = 60;
			return;
		}
		api.keyframes(p)
			.then(([k, d]) => {
				keyframes = k;
				duration = d;
				inPt = 0;
				outPt = d;
			})
			.catch((e) => toast('error', `Can't read this clip: ${e}`));
	});

	// Where the export will really start: the keyframe at or before the in point.
	const snappedIn = $derived(keyframes.filter((k) => k <= inPt + 1e-6).at(-1) ?? 0);
	const pct = (t: number) => `${(t / Math.max(duration, 0.001)) * 100}%`;

	function timeAt(e: PointerEvent) {
		const r = track!.getBoundingClientRect();
		return Math.min(duration, Math.max(0, ((e.clientX - r.left) / r.width) * duration));
	}

	function down(e: PointerEvent) {
		const t = timeAt(e);
		const r = track!.getBoundingClientRect();
		const px = (x: number) => (x / duration) * r.width;
		const near = (x: number) => Math.abs(px(x) - px(t)) < 8;
		drag = near(inPt) ? 'in' : near(outPt) ? 'out' : 'head';
		track!.setPointerCapture(e.pointerId);
		move(e);
	}

	function move(e: PointerEvent) {
		if (!drag) return;
		const t = timeAt(e);
		if (drag === 'in') inPt = Math.min(t, outPt - 0.5);
		else if (drag === 'out') outPt = Math.max(t, inPt + 0.5);
		if (video) video.currentTime = drag === 'head' ? t : drag === 'in' ? inPt : outPt;
	}

	function keys(e: KeyboardEvent) {
		if ((e.target as HTMLElement).tagName === 'INPUT') return;
		if (e.key === ' ') {
			e.preventDefault();
			paused = !paused;
		} else if (e.key === 'i') inPt = Math.min(current, outPt - 0.5);
		else if (e.key === 'o') outPt = Math.max(current, inPt + 0.5);
		else if (e.key === 'ArrowLeft' && video) video.currentTime = Math.max(0, current - (e.shiftKey ? 5 : 1 / 60));
		else if (e.key === 'ArrowRight' && video) video.currentTime = Math.min(duration, current + (e.shiftKey ? 5 : 1 / 60));
	}

	async function exportCut() {
		if (!inApp) return;
		busy = true;
		try {
			const out = await api.trim(path, inPt, outPt);
			await refreshClips();
			toast('success', `Saved "${out.name}"`);
			goto(`/trim?path=${encodeURIComponent(out.path)}`);
		} catch (e) {
			toast('error', String(e));
		} finally {
			busy = false;
		}
	}

	let exporting = $state(false);
	async function exportAs(preset: ExportPreset) {
		if (!inApp) return;
		exporting = true;
		try {
			const out = await api.exportClip(path, preset);
			await refreshClips();
			toast('success', `Exported ${out.split(/[\/]/).pop()}`);
		} catch (e) {
			toast('error', `Export failed: ${e}`);
		} finally {
			exporting = false;
		}
	}

	async function rename() {
		if (!inApp || !clip || name.trim() === clip.name || !name.trim()) return;
		try {
			const out = await api.renameClip(clip.path, name.trim());
			await refreshClips();
			goto(`/trim?path=${encodeURIComponent(out.path)}`, { replaceState: true });
		} catch (e) {
			toast('error', String(e));
			name = clip.name;
		}
	}

	async function remove() {
		if (!inApp || !clip) return;
		await api.deleteClips([clip.path]);
		toast('success', 'Moved to the Recycle Bin');
		await refreshClips();
		goto('/library');
	}
</script>

<svelte:window onkeydown={keys} />

<div class="page">
	<nav class="crumbs"><a href="/library">Clips</a></nav>

	<div class="top">
		<input
			class="title"
			bind:value={name}
			onblur={rename}
			onkeydown={(e) => e.key === 'Enter' && (e.currentTarget as HTMLInputElement).blur()}
			aria-label="Clip name"
			spellcheck="false"
		/>
		{#if clip}
			<p class="meta">
				<span class="num">{fmtDuration(clip.duration)}</span>, <span class="num">{fmtBytes(clip.size)}</span>,
				saved {fmtWhen(clip.modified)}
			</p>
		{/if}
	</div>

	<div class="player">
		{#if inApp && path}
			<!-- svelte-ignore a11y_media_has_caption -->
			<video
				bind:this={video}
				src={fileUrl(path)}
				bind:currentTime={current}
				bind:paused
				onclick={() => (paused = !paused)}
				preload="auto"
			></video>
		{:else}
			<div class="placeholder">Preview plays here inside the app.</div>
		{/if}
	</div>

	<div class="scrub">
		<div
			class="track"
			bind:this={track}
			onpointerdown={down}
			onpointermove={move}
			onpointerup={() => (drag = null)}
			role="slider"
			tabindex="0"
			aria-label="Playhead"
			aria-valuemin={0}
			aria-valuemax={duration}
			aria-valuenow={current}
		>
			<div class="sel" style="left: {pct(inPt)}; right: calc(100% - {pct(outPt)})"></div>
			{#if snappedIn < inPt - 0.05}
				<div class="snap" style="left: {pct(snappedIn)}; width: calc({pct(inPt)} - {pct(snappedIn)})"></div>
			{/if}
			{#each keyframes as k (k)}
				<span class="tick" style="left: {pct(k)}"></span>
			{/each}
			<span class="handle" style="left: {pct(inPt)}" aria-hidden="true"></span>
			<span class="handle out" style="left: {pct(outPt)}" aria-hidden="true"></span>
			<span class="head" style="left: {pct(current)}" aria-hidden="true"></span>
		</div>
		<div class="times num">
			<span>{fmtDuration(current)}</span>
			<span>{fmtDuration(duration)}</span>
		</div>
	</div>

	<div class="bar">
		<div class="range">
			<div><span class="k">In</span> <span class="num">{inPt.toFixed(1)}s</span></div>
			<div><span class="k">Out</span> <span class="num">{outPt.toFixed(1)}s</span></div>
			<div><span class="k">Length</span> <span class="num">{(outPt - snappedIn).toFixed(1)}s</span></div>
			<p class="explain">
				{#if snappedIn < inPt - 0.05}
					Starts at the keyframe at <span class="num">{snappedIn.toFixed(1)}s</span> so it saves instantly with no quality loss.
				{:else}
					Press <kbd>I</kbd> and <kbd>O</kbd> to set in and out at the playhead, <kbd>Space</kbd> to play.
				{/if}
			</p>
		</div>
		<div class="buttons">
			<button class="secondary" onclick={() => clip && inApp && revealItemInDir(clip.path)}>Show in folder</button>
			<button class="secondary" onclick={remove}>Move to Recycle Bin</button>
			<ExportMenu busy={exporting} onexport={exportAs} />
			<button class="primary" disabled={busy || (inPt <= 0.01 && outPt >= duration - 0.01)} onclick={exportCut}>
				{busy ? 'Saving…' : 'Save trimmed copy'}
			</button>
		</div>
	</div>
</div>

<style>
	.page {
		padding: 16px 32px 32px;
		display: grid;
		gap: 16px;
		max-width: 1200px;
	}
	.crumbs a {
		color: var(--muted);
		text-decoration: none;
		font-size: 13px;
	}
	.crumbs a:hover {
		color: var(--text);
	}
	.title {
		width: 100%;
		border: 0;
		background: transparent;
		font-size: 24px;
		font-weight: 650;
		letter-spacing: -0.02em;
		padding: 0;
		outline: none;
	}
	.title:focus {
		box-shadow: 0 1px 0 var(--muted);
	}
	.meta {
		margin: 4px 0 0;
		color: var(--muted);
	}
	.player {
		background: var(--media);
		border-radius: 4px;
		overflow: hidden;
		aspect-ratio: 16 / 9;
		max-height: 56vh;
		display: grid;
		place-items: center;
	}
	video {
		width: 100%;
		height: 100%;
		object-fit: contain;
		display: block;
	}
	.placeholder {
		color: var(--faint);
	}
	.track {
		position: relative;
		height: 56px;
		background: var(--sunken);
		border-radius: 4px;
		cursor: pointer;
		touch-action: none;
	}
	.sel {
		position: absolute;
		top: 0;
		bottom: 0;
		background: var(--raised);
	}
	.snap {
		position: absolute;
		top: 0;
		bottom: 0;
		background: var(--raised);
		opacity: 0.5;
	}
	.tick {
		position: absolute;
		bottom: 0;
		width: 1px;
		height: 8px;
		background: var(--line-strong);
	}
	.handle {
		position: absolute;
		top: 0;
		bottom: 0;
		width: 4px;
		margin-left: -2px;
		background: var(--accent);
		border-radius: 2px;
	}
	.head {
		position: absolute;
		top: -4px;
		bottom: -4px;
		width: 2px;
		margin-left: -1px;
		background: var(--muted);
		pointer-events: none;
	}
	.times {
		display: flex;
		justify-content: space-between;
		color: var(--faint);
		font-size: 11px;
		margin-top: 8px;
	}
	.bar {
		display: flex;
		justify-content: space-between;
		align-items: flex-start;
		gap: 24px;
	}
	.range {
		display: flex;
		flex-wrap: wrap;
		gap: 8px 24px;
		align-items: baseline;
	}
	.k {
		color: var(--faint);
		font-size: 12px;
		margin-right: 4px;
	}
	.explain {
		flex-basis: 100%;
		margin: 0;
		color: var(--muted);
		font-size: 13px;
	}
	.buttons {
		display: flex;
		gap: 8px;
		flex: none;
	}
	.primary,
	.secondary {
		height: 36px;
		padding: 0 16px;
		border: 0;
		border-radius: 4px;
		font-weight: 600;
		cursor: pointer;
	}
	.primary {
		background: var(--accent);
		color: var(--on-accent);
	}
	.primary:disabled {
		background: var(--line);
		color: var(--faint);
		cursor: default;
	}
	.secondary {
		background: var(--raised);
	}
	.secondary:hover {
		background: var(--line);
	}
	kbd {
		font-family: var(--font-mono);
		font-size: 11px;
		padding: 1px 6px;
		border-radius: 4px;
		background: var(--raised);
		color: var(--text);
	}
</style>
