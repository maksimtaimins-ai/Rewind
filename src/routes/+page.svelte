<script lang="ts">
	import { api, fmtDuration, type Clip } from '#lib/api.ts';
	import { app, inApp, toast } from '#lib/stores/app.svelte.ts';
	import Tape from '#lib/components/Tape.svelte';
	import StatusLine from '#lib/components/StatusLine.svelte';
	import ClipTile from '#lib/components/ClipTile.svelte';
	import { goto } from '$app/navigation';

	const s = $derived(app.status);
	const settings = $derived(app.settings!);
	const live = $derived(s?.state === 'recording' || s?.state === 'paused');
	const capacity = $derived(s?.buffer_secs ?? settings.buffer.seconds);
	// The buffer keeps a little slack past its length; show what a save would hold.
	const held = $derived(Math.min(capacity, live ? (s?.buffered_secs ?? 0) : 0));
	const recent = $derived(app.clips.slice(0, 3));

	async function run(fn: () => Promise<unknown>) {
		if (!inApp) return;
		try {
			await fn();
		} catch (e) {
			toast('error', String(e));
		}
	}

	const hotkeys = $derived(
		[
			{ keys: settings.hotkeys.save_clip, what: `Save the last ${fmtDuration(settings.buffer.seconds)}` },
			{ keys: settings.hotkeys.marker, what: 'Mark this moment' },
			{ keys: settings.hotkeys.screenshot, what: 'Screenshot' },
			{ keys: settings.hotkeys.extend_last, what: `Add ${settings.buffer.extend_secs}s to the last clip` }
		].filter((h): h is { keys: string; what: string } => !!h && !!h.keys)
	);

	const open = (c: Clip) => goto(`/trim?path=${encodeURIComponent(c.path)}`);
</script>

<div class="page">
	<section class="head">
		<div class="who">
			<StatusLine status={s} />
			<h1>{s?.game ?? 'Desktop'}</h1>
			<p class="lede">
				The replay buffer runs the whole time Rewind is open. Nothing is kept until you save.
			</p>
		</div>
		<div class="actions">
			<button class="primary" onclick={() => run(() => api.saveNow())}>Save clip</button>
		</div>
	</section>

	<section class="buffer" aria-label="Replay buffer">
		<div class="counter">
			<span class="rec" aria-hidden="true"></span>
			<span class="held num" aria-live="off">Buffer: {Math.floor(held)}s / {capacity}s</span>
			<span class="of">held in memory</span>
		</div>
		<Tape
			{capacity}
			buffered={held}
			markers={live ? (s?.markers ?? []) : []}
			recording={s?.state === 'recording'}
			keyframeSecs={settings.video.keyframe_secs}
			onmarker={(i) => run(() => api.saveMarker(i, 20))}
		/>
		{#if live && s?.markers.length}
			<p class="note">Click a marker to save the 20 seconds leading up to it.</p>
		{/if}
	</section>

	<section class="facts" aria-label="Capture details">
		<dl>
			<div>
				<dt>Encoder</dt>
				<dd>{s?.encoder?.replace(' Encoder MFT', '') ?? '—'}</dd>
			</div>
			<div>
				<dt>Output</dt>
				<dd class="num">{s?.resolution ? `${s.resolution[0]}×${s.resolution[1]}` : '—'}</dd>
			</div>
			<div>
				<dt>Frame rate</dt>
				<dd class="num">{live ? `${(s?.fps ?? 0).toFixed(0)} / ${settings.video.fps}` : '—'}</dd>
			</div>
			<div>
				<dt>Bitrate</dt>
				<dd class="num">{live ? `${((s?.bitrate_kbps ?? 0) / 1000).toFixed(1)} Mb/s` : '—'}</dd>
			</div>
			<div>
				<dt>Memory</dt>
				<dd class="num">{live ? `${(s?.memory_mb ?? 0).toFixed(0)} MB` : '—'}</dd>
			</div>
			<div>
				<dt>Dropped</dt>
				<dd class="num">{live ? (s?.dropped ?? 0) : '—'}</dd>
			</div>
		</dl>
	</section>

	<div class="lower">
		<section class="keys" aria-labelledby="keys-h">
			<h2 id="keys-h">Hotkeys</h2>
			<ul>
				{#each hotkeys as h (h.what)}
					<li>
						<span class="combo">
							{#each h.keys.split('+') as k, i (i)}<kbd>{k}</kbd>{/each}
						</span>
						<span>{h.what}</span>
					</li>
				{/each}
			</ul>
			<a href="/settings#hotkeys" class="quiet">Change hotkeys</a>
		</section>

		<section class="recent" aria-labelledby="recent-h">
			<div class="recent-head">
				<h2 id="recent-h">Latest clips</h2>
				{#if app.clips.length > 3}<a href="/library" class="quiet">All {app.clips.length} clips</a>{/if}
			</div>
			{#if recent.length}
				<div class="grid">
					{#each recent as c (c.path)}
						<ClipTile clip={c} onopen={() => open(c)} onselect={() => open(c)} />
					{/each}
				</div>
			{:else}
				<p class="empty">
					Clips you save show up here. Press
					{#if settings.hotkeys.save_clip}<kbd>{settings.hotkeys.save_clip}</kbd>{/if} while playing.
				</p>
			{/if}
		</section>
	</div>
</div>

<style>
	.page {
		padding: 16px 32px 32px;
		display: grid;
		gap: 32px;
		max-width: 1200px;
	}
	.head {
		display: flex;
		justify-content: space-between;
		align-items: flex-start;
		gap: 24px;
	}
	h1 {
		margin: 8px 0 0;
		font-size: 32px;
		line-height: 1.1;
		font-weight: 650;
		letter-spacing: -0.02em;
	}
	.lede {
		margin: 8px 0 0;
		color: var(--muted);
		max-width: 56ch;
	}
	.actions {
		display: flex;
		gap: 8px;
		flex: none;
		padding-top: 24px;
	}
	button.primary {
		height: 36px;
		padding: 0 16px;
		border-radius: 4px;
		border: 0;
		font-weight: 600;
		cursor: pointer;
	}
	.primary {
		background: var(--text);
		color: var(--ground);
	}

	.counter {
		display: flex;
		align-items: center;
		gap: 12px;
		margin-bottom: 12px;
	}
	/* Always on: the buffer never stops while the app is open. */
	.rec {
		flex: none;
		width: 14px;
		height: 14px;
		border-radius: 50%;
		background: var(--signal);
	}
	.held {
		font-size: 40px;
		line-height: 1;
		font-weight: 500;
		letter-spacing: -0.04em;
	}
	.of {
		color: var(--muted);
	}
	.note {
		margin: 0;
		color: var(--muted);
		font-size: 13px;
	}

	dl {
		margin: 0;
		display: grid;
		grid-template-columns: repeat(6, minmax(0, 1fr));
		border-top: 1px solid var(--line);
	}
	dl div {
		padding: 12px 16px 0 0;
		min-width: 0;
	}
	dt {
		color: var(--faint);
		font-size: 12px;
	}
	dd {
		margin: 4px 0 0;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.lower {
		display: grid;
		grid-template-columns: minmax(240px, 1fr) 2fr;
		gap: 32px;
	}
	h2 {
		margin: 0 0 12px;
		font-size: 14px;
		font-weight: 600;
	}
	.keys ul {
		list-style: none;
		margin: 0 0 12px;
		padding: 0;
		display: grid;
		gap: 8px;
	}
	.keys li {
		display: grid;
		grid-template-columns: 112px 1fr;
		gap: 12px;
		align-items: center;
		color: var(--muted);
	}
	.combo {
		display: flex;
		gap: 4px;
	}
	kbd {
		font-family: var(--font-mono);
		font-size: 11px;
		padding: 2px 6px;
		border-radius: 4px;
		background: var(--raised);
		color: var(--text);
	}
	.quiet {
		color: var(--muted);
		font-size: 13px;
	}
	.quiet:hover {
		color: var(--text);
	}
	.recent-head {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 16px;
	}
	.empty {
		color: var(--muted);
		margin: 0;
	}

	@media (max-width: 1000px) {
		dl {
			grid-template-columns: repeat(3, minmax(0, 1fr));
		}
		.lower {
			grid-template-columns: 1fr;
		}
	}
</style>
