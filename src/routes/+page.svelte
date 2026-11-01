<script lang="ts">
	import { api, fileUrl, fmtDuration, fmtWhen, type HotkeyAction } from '#lib/api.ts';
	import { app, inApp, toast } from '#lib/stores/app.svelte.ts';
	import Tape from '#lib/components/Tape.svelte';
	import MarkerList from '#lib/components/MarkerList.svelte';
	import { revealItemInDir } from '@tauri-apps/plugin-opener';
	import { goto } from '$app/navigation';

	const s = $derived(app.status);
	const settings = $derived(app.settings!);
	const live = $derived(s?.state === 'recording' || s?.state === 'paused');
	const capacity = $derived(s?.buffer_secs ?? settings.buffer.seconds);
	// The buffer keeps a little slack past its length; show what a save would hold.
	const held = $derived(Math.min(capacity, live ? (s?.buffered_secs ?? 0) : 0));
	const extraSlots = $derived((s?.slots ?? []).slice(1));
	// The tape spans the longest slot, since that's how far back the buffer reaches.
	const reach = $derived(Math.max(capacity, ...extraSlots.map((x) => x.seconds)));
	const markers = $derived(s?.markers ?? []);
	const onTape = $derived(live ? markers.filter((m) => m.in_buffer) : []);
	const last = $derived(app.lastSaved ?? app.clips[0] ?? null);
	const lastThumb = $derived(last?.thumb && inApp ? `${fileUrl(last.thumb)}?v=${last.thumb_rev}` : null);
	let details = $state(false);
	let showMarkers = $state(false);

	const clipCopy = $derived(
		`Clip: last ${capacity}s` + (settings.post_roll.enabled ? ` + ${settings.post_roll.seconds}s after` : '')
	);
	const stateCopy = $derived.by(() => {
		switch (s?.state) {
			case 'paused':
				return 'Paused while a private app is focused';
			case 'error':
				return s.error ?? 'Capture failed';
			case 'recording':
				return null;
			default:
				return 'Starting capture';
		}
	});

	async function run(fn: () => Promise<unknown>) {
		if (!inApp) return;
		try {
			await fn();
		} catch (e) {
			toast('error', String(e));
		}
	}

	type Tool = { action: HotkeyAction; label: string; keys: string; icon: string };
	const tools = $derived<Tool[]>([
		{ action: 'marker', label: 'Mark this moment', keys: settings.hotkeys.marker, icon: 'marker' },
		{ action: 'screenshot', label: 'Screenshot', keys: settings.hotkeys.screenshot, icon: 'camera' },
		{
			action: 'slow_mo',
			label: `Slow-mo of the last ${settings.slow_mo.seconds}s`,
			keys: settings.hotkeys.slow_mo,
			icon: 'slow'
		},
		{ action: 'extend_last', label: `Add ${settings.buffer.extend_secs}s before the last clip`, keys: settings.hotkeys.extend_last, icon: 'extend' },
		{ action: 'favorite_last', label: 'Favorite the last clip', keys: settings.hotkeys.favorite_last, icon: 'star' }
	]);

	const open = (path: string) => goto(`/trim?path=${encodeURIComponent(path)}`);
</script>

<div class="page">
	{#if s && s.disk !== 'ok' && s.disk_free_gb != null}
		<p class="disk {s.disk}" role="status">
			{#if s.disk === 'full'}
				<strong>Only {s.disk_free_gb.toFixed(1)} GB free on the clips drive.</strong> Saving clips is paused until there's more than {settings.disk.block_gb} GB. Screenshots still work. <a href="/settings#storage">Storage</a>
			{:else}
				<strong>Only {s.disk_free_gb.toFixed(1)} GB free on the clips drive.</strong> Clips still save; below {settings.disk.block_gb} GB they'll stop. <a href="/settings#storage">Storage</a>
			{/if}
		</p>
	{/if}
	<div class="core">
		<p class="context">{s?.game ?? 'Desktop'}</p>

		<button class="save" onclick={() => run(() => api.saveNow())}>
			<span>Save clip</span>
			{#if settings.hotkeys.save_clip}<span class="keys num">{settings.hotkeys.save_clip}</span>{/if}
		</button>

		<p class="buffer" aria-live="off">
			<span class="dot {s?.state ?? 'idle'}" aria-hidden="true"></span>
			{#if stateCopy}
				<span class="state">{stateCopy}</span>
			{:else}
				<span class="num">Buffer: {Math.floor(held)}s / {capacity}s</span>
				<span class="clip">{clipCopy}</span>
			{/if}
		</p>

		<div class="tools" role="toolbar" aria-label="More actions">
			{#each tools as t (t.action)}
				<button class="tool" aria-label="{t.label}{t.keys ? ` (${t.keys})` : ''}" onclick={() => run(() => api.runAction(t.action))}>
					<svg viewBox="0 0 24 24" aria-hidden="true">
						{#if t.icon === 'marker'}
							<path d="M6 21V4h11l-2.5 4.5L17 13H6" />
						{:else if t.icon === 'camera'}
							<path d="M4 8h3l2-3h6l2 3h3v11H4z" /><circle cx="12" cy="13" r="3.5" />
						{:else if t.icon === 'slow'}
							<circle cx="12" cy="12" r="8" /><path d="M12 7v5l3 2" />
						{:else if t.icon === 'extend'}
							<path d="M12 7l-5 5 5 5M19 7l-5 5 5 5" />
						{:else}
							<path d="M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z" />
						{/if}
					</svg>
					<span class="tip" role="tooltip">
						{t.label}{#if t.keys}<span class="num">{t.keys}</span>{/if}
					</span>
				</button>
			{/each}
			<button class="more" aria-expanded={details} onclick={() => (details = !details)}>
				{details ? 'Hide details' : 'Details'}{#if markers.length}<span class="num count">{markers.length}</span>{/if}
			</button>
		</div>
	</div>

	{#if details}
		<section class="details" aria-label="Buffer details">
			<Tape
				capacity={reach}
				buffered={Math.min(reach, live ? (s?.buffered_secs ?? 0) : 0)}
				markers={onTape.map((m) => m.ago)}
				recording={s?.state === 'recording'}
				keyframeSecs={settings.video.keyframe_secs}
				onmarker={(i) => run(() => api.saveMarker(onTape[i].id, 30))}
			/>
			{#if onTape.length && !showMarkers}
				<p class="note">Click a marker to save 30 seconds around it.</p>
			{/if}

			{#if extraSlots.length}
				<ul class="slots">
					{#each extraSlots as x (x.name)}
						<li>
							<span>Slot {x.name}</span>
							<span class="meter" aria-hidden="true"><span style="width: {(x.held / x.seconds) * 100}%"></span></span>
							<span class="num">{Math.floor(x.held)}s / {x.seconds}s</span>
							<span class="num faint">{x.hotkey}</span>
							<button class="link" onclick={() => run(() => api.saveNow(x.seconds))}>Save</button>
						</li>
					{/each}
				</ul>
			{/if}

			{#if markers.length}
				<button class="link" aria-expanded={showMarkers} onclick={() => (showMarkers = !showMarkers)}>
					{showMarkers ? 'Hide markers' : `Show ${markers.length} ${markers.length === 1 ? 'marker' : 'markers'}`}
				</button>
				{#if showMarkers}
					<MarkerList {markers} maxSecs={capacity} onclip={(id, secs) => run(() => api.saveMarker(id, secs))} />
				{/if}
			{/if}

			<dl>
				<div><dt>Encoder</dt><dd>{s?.encoder?.replace(' Encoder MFT', '') ?? '—'}</dd></div>
				<div><dt>Output</dt><dd class="num">{s?.resolution ? `${s.resolution[0]}×${s.resolution[1]}` : '—'}</dd></div>
				<div><dt>Frame rate</dt><dd class="num">{live ? `${(s?.fps ?? 0).toFixed(0)} / ${settings.video.fps}` : '—'}</dd></div>
				<div><dt>Bitrate</dt><dd class="num">{live ? `${((s?.bitrate_kbps ?? 0) / 1000).toFixed(1)} Mb/s` : '—'}</dd></div>
				<div><dt>Memory</dt><dd class="num">{live ? `${(s?.memory_mb ?? 0).toFixed(0)} MB` : '—'}</dd></div>
				<div><dt>Dropped</dt><dd class="num">{live ? (s?.dropped ?? 0) : '—'}</dd></div>
			</dl>
		</section>
	{/if}

	<aside class="last" aria-label="Last saved clip">
		{#if last}
			<button class="thumb" onclick={() => open(last.path)} aria-label="Open {last.name}">
				{#if lastThumb}<img src={lastThumb} alt="" />{/if}
				<span class="len num">{fmtDuration(last.duration)}</span>
			</button>
			<div class="last-meta">
				<span class="last-name" title={last.name}>{last.name}</span>
				<span class="faint">{fmtWhen(last.modified)}</span>
				<button class="link" onclick={() => inApp && revealItemInDir(last.path)}>Open folder</button>
			</div>
		{:else}
			<p class="faint">Saved clips show up here.</p>
		{/if}
	</aside>
</div>

<style>
	.page {
		position: relative;
		min-height: 100%;
		box-sizing: border-box;
		padding: 32px;
		display: flex;
		flex-direction: column;
		gap: 32px;
	}
	.disk {
		margin: -8px 0 -8px;
		padding: 10px 14px;
		border-radius: 6px;
		color: var(--text);
		max-width: 72ch;
		background: color-mix(in srgb, #f5a524 16%, var(--raised));
		box-shadow: inset 3px 0 0 #f5a524;
	}
	.disk a {
		color: inherit;
	}
	.disk.full {
		background: color-mix(in srgb, var(--danger) 18%, var(--raised));
		box-shadow: inset 3px 0 0 var(--danger);
	}
	.core {
		display: grid;
		gap: 16px;
		justify-items: start;
		max-width: 480px;
	}
	.context {
		margin: 0;
		color: var(--muted);
	}

	/* The one primary element on the page. */
	.save {
		width: 100%;
		height: 72px;
		padding: 0 24px;
		display: flex;
		align-items: center;
		justify-content: space-between;
		border: 0;
		border-radius: 8px;
		background: var(--accent);
		color: var(--on-accent);
		font-size: 24px;
		font-weight: 650;
		letter-spacing: -0.02em;
		cursor: pointer;
	}
	.save:hover {
		filter: brightness(1.08);
	}
	.save:active {
		transform: translateY(1px);
	}
	.keys {
		font-size: 12px;
		font-weight: 500;
		opacity: 0.6;
	}

	.buffer {
		margin: 0;
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
	}
	.clip,
	.state {
		color: var(--muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.clip {
		margin-left: 8px;
	}
	.dot {
		flex: none;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--faint);
	}
	.dot.recording {
		background: var(--signal);
	}
	.dot.paused {
		background: transparent;
		box-shadow: inset 0 0 0 2px var(--muted);
	}

	.tools {
		display: flex;
		align-items: center;
		gap: 4px;
	}
	.tool {
		position: relative;
		width: 32px;
		height: 32px;
		display: grid;
		place-items: center;
		border: 0;
		border-radius: 4px;
		background: transparent;
		color: var(--muted);
		cursor: pointer;
	}
	.tool:hover,
	.tool:focus-visible {
		background: var(--raised);
		color: var(--text);
	}
	.tool svg {
		width: 18px;
		height: 18px;
		fill: none;
		stroke: currentColor;
		stroke-width: 1.6;
		stroke-linejoin: round;
		stroke-linecap: round;
	}
	.tip {
		position: absolute;
		left: 0;
		top: calc(100% + 8px);
		display: none;
		gap: 8px;
		padding: 4px 8px;
		border-radius: 4px;
		background: var(--text);
		color: var(--ground);
		font-size: 12px;
		white-space: nowrap;
		pointer-events: none;
		z-index: 10;
	}
	.tool:hover .tip,
	.tool:focus-visible .tip {
		display: flex;
	}
	.tip .num {
		opacity: 0.7;
	}
	.more {
		margin-left: 8px;
		height: 32px;
		padding: 0 8px;
		border: 0;
		border-radius: 4px;
		background: transparent;
		color: var(--muted);
		font-size: 13px;
		cursor: pointer;
	}
	.more:hover,
	.more[aria-expanded='true'] {
		color: var(--text);
	}
	.count {
		margin-left: 8px;
		padding: 0 4px;
		border-radius: 4px;
		background: var(--raised);
	}

	.details {
		display: grid;
		gap: 16px;
		max-width: 960px;
	}
	.note {
		margin: 0;
		color: var(--muted);
		font-size: 13px;
	}
	.slots {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 8px;
	}
	.slots li {
		display: grid;
		grid-template-columns: 56px 160px 96px 96px auto;
		align-items: center;
		gap: 12px;
		color: var(--muted);
	}
	.meter {
		height: 4px;
		border-radius: 2px;
		background: var(--sunken);
		overflow: hidden;
	}
	.meter span {
		display: block;
		height: 100%;
		background: var(--line-strong);
	}
	.link {
		justify-self: start;
		padding: 0;
		border: 0;
		background: none;
		color: var(--muted);
		font-size: 13px;
		cursor: pointer;
	}
	.link:hover {
		color: var(--text);
	}
	dl {
		margin: 0;
		display: grid;
		grid-template-columns: repeat(6, minmax(0, 1fr));
		gap: 16px;
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
	.faint {
		color: var(--faint);
		font-size: 12px;
	}

	.last {
		margin-top: auto;
		align-self: flex-end;
		width: 240px;
		display: grid;
		gap: 8px;
	}
	.thumb {
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
	.thumb img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}
	.len {
		position: absolute;
		right: 4px;
		bottom: 4px;
		padding: 0 4px;
		border-radius: 4px;
		background: var(--scrim);
		color: var(--on-scrim);
		font-size: 11px;
	}
	.last-meta {
		display: grid;
		gap: 4px;
		min-width: 0;
	}
	.last-name {
		font-size: 13px;
		font-weight: 500;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	@media (max-width: 1000px) {
		dl {
			grid-template-columns: repeat(3, minmax(0, 1fr));
		}
	}
</style>
