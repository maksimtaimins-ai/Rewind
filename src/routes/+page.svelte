<script lang="ts">
	import { api, fileUrl, fmtBytes, fmtDuration, type Clip, type HotkeyAction } from '#lib/api.ts';
	import { app, inApp, toast } from '#lib/stores/app.svelte.ts';
	import BufferTimeline from '#lib/components/BufferTimeline.svelte';
	import Keycap from '#lib/components/Keycap.svelte';
	import MarkerList from '#lib/components/MarkerList.svelte';
	import { goto } from '$app/navigation';

	const s = $derived(app.status);
	const settings = $derived(app.settings!);
	const live = $derived(s?.state === 'recording' || s?.state === 'paused');
	const capacity = $derived(s?.buffer_secs ?? settings.buffer.seconds);
	const held = $derived(Math.min(capacity, live ? (s?.buffered_secs ?? 0) : 0));
	const extraSlots = $derived((s?.slots ?? []).slice(1));
	const markers = $derived(s?.markers ?? []);
	const onTape = $derived(live ? markers.filter((m) => m.in_buffer) : []);
	let showMarkers = $state(false);

	const stateCopy = $derived.by(() => {
		switch (s?.state) {
			case 'paused':
				return 'Paused while a private app is focused. Nothing is being kept.';
			case 'error':
				return s.error ?? 'Capture failed';
			case 'recording':
				return null;
			default:
				return 'Starting capture';
		}
	});
	const keepCopy = $derived(
		`Keeps the last ${capacity} seconds` + (settings.post_roll.enabled ? `, plus ${settings.post_roll.seconds} after you press save` : '')
	);

	async function run(fn: () => Promise<unknown>) {
		if (!inApp) return;
		try {
			await fn();
		} catch (e) {
			toast('error', String(e));
		}
	}

	// Save feedback: the check draws in once the save is accepted, and the
	// timeline lights up the stretch that was kept when the clip lands.
	let saved = $state(0);
	let flash = $state(0);
	let flashSecs = $state(30);
	let seen: Clip | null = app.lastSaved;
	$effect(() => {
		const c = app.lastSaved;
		if (c && c !== seen) {
			seen = c;
			flashSecs = c.duration;
			flash++;
		}
	});
	async function save(secs?: number) {
		if (!inApp) {
			// Browser preview: act out a save.
			flashSecs = secs ?? capacity;
			flash++;
			toast('success', `Saved ${secs ?? capacity}s clip`);
		} else {
			try {
				await api.saveNow(secs);
			} catch (e) {
				toast('error', String(e));
				return;
			}
		}
		const n = ++saved;
		setTimeout(() => saved === n && (saved = 0), 1400);
	}

	type Tool = { action: HotkeyAction; label: string; hint: string; keys: string; icon: string };
	const tools = $derived<Tool[]>([
		{ action: 'marker', label: 'Add marker', hint: 'Flag this moment to clip it later', keys: settings.hotkeys.marker, icon: 'marker' },
		{ action: 'screenshot', label: 'Screenshot', hint: 'Save a still of your screen', keys: settings.hotkeys.screenshot, icon: 'camera' },
		{ action: 'slow_mo', label: 'Save slow-mo', hint: `The last ${settings.slow_mo.seconds}s at ${settings.slow_mo.speed}× speed`, keys: settings.hotkeys.slow_mo, icon: 'slow' },
		{ action: 'extend_last', label: 'Extend last', hint: `Add ${settings.buffer.extend_secs}s before the last clip`, keys: settings.hotkeys.extend_last, icon: 'extend' }
	]);

	const open = (path: string) => goto(`/trim?path=${encodeURIComponent(path)}`);
	const recent = $derived(app.clips.slice(0, 5));
	const thumbOf = (c: Clip) => (c.thumb && inApp ? `${fileUrl(c.thumb)}?v=${c.thumb_rev}` : null);

	// Stats, from the clip list.
	const DAY = 864e5;
	const startOfDay = (ms: number) => new Date(new Date(ms).toDateString()).getTime();
	const today = $derived(startOfDay(Date.now()));
	const monday = $derived(today - ((new Date(today).getDay() + 6) % 7) * DAY);
	const bins = (from: number, width: number, n: number) => {
		const out = new Array(n).fill(0);
		for (const c of app.clips) {
			const i = Math.floor((c.modified - from) / width);
			if (i >= 0 && i < n) out[i]++;
		}
		return out;
	};
	const todayBins = $derived(bins(today, DAY / 12, 12));
	const weekBins = $derived(bins(monday, DAY, 7));
	const monthBins = $derived(bins(today - 29 * DAY, DAY, 30));
	const usedBytes = $derived(app.clips.reduce((n, c) => n + c.size, 0));
	const freeGb = $derived(s?.disk_free_gb ?? null);
	const usedShare = $derived(freeGb == null ? 0 : usedBytes / 1e9 / (usedBytes / 1e9 + freeGb));
	const sum = (a: number[]) => a.reduce((n, x) => n + x, 0);

	/** Bars for a sparkline, as SVG rects in a 60x20 box. */
	const bars = (data: number[]) => {
		const max = Math.max(1, ...data);
		const w = 60 / data.length;
		return data.map((v, i) => ({ x: i * w + w * 0.15, w: w * 0.7, h: v ? Math.max(2, (v / max) * 20) : 1, v }));
	};

	const encoderName = $derived(s?.encoder?.replace(/ Encoder MFT$/, '').replace(/ Encoder$/, '') ?? null);
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

	<section class="hero" aria-label="Replay buffer">
		<header class="hero-head">
			<h1>{s?.game ?? 'Desktop'}</h1>
			<p class="keep">
				{#if stateCopy}{stateCopy}{:else}{keepCopy}<span class="num held">{Math.floor(held)}s held</span>{/if}
			</p>
		</header>
		<div data-tour="timeline">
			<BufferTimeline
				{capacity}
				{held}
				levels={s?.levels ?? []}
				state={s?.state ?? 'idle'}
				markers={onTape}
				{flash}
				{flashSecs}
				onmarker={(id) => run(() => api.saveMarker(id, 30))}
			/>
		</div>
	</section>

	<div class="saves">
		<button class="save" class:done={saved > 0} onclick={() => save()} data-tour="save">
			<span class="mark" aria-hidden="true">
				{#key saved}
					{#if saved}
						<svg viewBox="0 0 24 24" class="check"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>
					{:else}
						<svg viewBox="0 0 24 24" class="rewind"><path d="M3 12l8-6v12zM12 12l8-6v12z" /></svg>
					{/if}
				{/key}
			</span>
			<span class="save-label">{saved ? 'Saved' : 'Save clip'}</span>
			<Keycap keys={settings.hotkeys.save_clip} tone="on-accent" />
		</button>
		{#each extraSlots as x (x.name)}
			<button class="slot" onclick={() => save(x.seconds)} title="Save the last {fmtDuration(x.seconds)}">
				<span>Save {x.seconds >= 60 ? `${x.seconds / 60} min` : `${x.seconds}s`}</span>
				<Keycap keys={x.hotkey} />
			</button>
		{/each}
	</div>

	<div class="tools" role="toolbar" aria-label="More actions">
		{#each tools as t (t.action)}
			<button class="tool" title={t.hint} onclick={() => run(() => api.runAction(t.action))}>
				<svg viewBox="0 0 24 24" aria-hidden="true">
					{#if t.icon === 'marker'}
						<path d="M6 21V4h11l-2.5 4.5L17 13H6" />
					{:else if t.icon === 'camera'}
						<path d="M4 8h3l2-3h6l2 3h3v11H4z" /><circle cx="12" cy="13" r="3.5" />
					{:else if t.icon === 'slow'}
						<circle cx="12" cy="12" r="8" /><path d="M12 7v5l3 2" />
					{:else}
						<path d="M12 7l-5 5 5 5M19 7l-5 5 5 5" />
					{/if}
				</svg>
				<span class="tool-label">{t.label}</span>
				<Keycap keys={t.keys} />
				{#if t.action === 'marker' && markers.length}
					<span class="badge num">{markers.length} {markers.length === 1 ? 'marker' : 'markers'}</span>
				{/if}
			</button>
		{/each}
	</div>

	{#if markers.length}
		<section class="markers" aria-label="Markers">
			<button class="link" aria-expanded={showMarkers} onclick={() => (showMarkers = !showMarkers)}>
				{showMarkers ? 'Hide markers' : `Show ${markers.length} ${markers.length === 1 ? 'marker' : 'markers'}`}
			</button>
			{#if showMarkers}
				<MarkerList {markers} maxSecs={capacity} onclip={(id, secs) => run(() => api.saveMarker(id, secs))} />
			{/if}
		</section>
	{/if}

	<section class="stats" aria-label="Clip stats">
		<div class="stat">
			<div class="stat-copy"><span class="stat-n num">{sum(todayBins)}</span><span class="stat-l">Clips today</span></div>
			<svg class="spark" viewBox="0 0 60 20" aria-hidden="true">
				{#each bars(todayBins) as b, i (i)}<rect x={b.x} y={20 - b.h} width={b.w} height={b.h} class:on={b.v > 0} />{/each}
			</svg>
		</div>
		<div class="stat">
			<div class="stat-copy"><span class="stat-n num">{sum(weekBins)}</span><span class="stat-l">This week</span></div>
			<svg class="spark" viewBox="0 0 60 20" aria-hidden="true">
				{#each bars(weekBins) as b, i (i)}<rect x={b.x} y={20 - b.h} width={b.w} height={b.h} class:on={b.v > 0} />{/each}
			</svg>
		</div>
		<div class="stat">
			<div class="stat-copy"><span class="stat-n num">{app.clips.length}</span><span class="stat-l">Total clips</span></div>
			<svg class="spark" viewBox="0 0 60 20" aria-hidden="true">
				{#each bars(monthBins) as b, i (i)}<rect x={b.x} y={20 - b.h} width={b.w} height={b.h} class:on={b.v > 0} />{/each}
			</svg>
		</div>
		<div class="stat">
			<div class="stat-copy">
				<span class="stat-n num">{fmtBytes(usedBytes)}</span>
				<span class="stat-l">{freeGb == null ? 'Used by clips' : `Used, ${freeGb.toFixed(1)} GB free`}</span>
			</div>
			<svg class="ring" class:low={s?.disk !== 'ok'} viewBox="0 0 24 24" aria-hidden="true">
				<circle cx="12" cy="12" r="9" class="ring-bg" />
				<circle cx="12" cy="12" r="9" class="ring-fg" pathLength="100" stroke-dasharray="{Math.max(2, usedShare * 100)} 100" />
			</svg>
		</div>
	</section>

	<section class="recent" aria-label="Recent clips">
		<div class="recent-head">
			<h2>Recent clips</h2>
			{#if app.clips.length > 5}<a href="/library" class="link">See all {app.clips.length}</a>{/if}
		</div>
		{#if recent.length}
			<ul class="strip">
				{#each recent as c (c.path)}
					{@const thumb = thumbOf(c)}
					<li>
						<button class="clip" onclick={() => open(c.path)} aria-label="Open {c.name}">
							{#if thumb}<img src={thumb} alt="" loading="lazy" />{/if}
							<span class="clip-over">
								<span class="clip-name">{c.name}</span>
								<span class="num">{fmtDuration(c.duration)}</span>
							</span>
							<span class="clip-len num">{fmtDuration(c.duration)}</span>
						</button>
					</li>
				{/each}
			</ul>
		{:else}
			<p class="empty">Saved clips show up here.</p>
		{/if}
	</section>

	<footer class="statusline num" aria-label="Capture details">
		<span>{encoderName ?? 'No encoder yet'}</span>
		<span>{live ? `${(s?.fps ?? 0).toFixed(0)} fps` : `${settings.video.fps} fps target`}</span>
		{#if s?.resolution}<span>{s.resolution[0]}×{s.resolution[1]}</span>{/if}
		<span>{s?.game ? `Game: ${s.game}` : 'Desktop'}</span>
		{#if live && s?.dropped}<span class="warn">{s.dropped} dropped</span>{/if}
	</footer>
</div>

<style>
	.page {
		min-height: 100%;
		box-sizing: border-box;
		padding: 24px 32px 16px;
		display: flex;
		flex-direction: column;
		gap: 20px;
		max-width: 1120px;
	}
	.disk {
		margin: 0;
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

	.hero {
		display: grid;
		gap: 12px;
	}
	.hero-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 16px;
		min-width: 0;
	}
	h1 {
		margin: 0;
		font-size: 22px;
		font-weight: 650;
		letter-spacing: -0.02em;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.keep {
		margin: 0;
		color: var(--muted);
		display: flex;
		gap: 12px;
		white-space: nowrap;
	}
	.held {
		color: var(--faint);
	}

	/* The one primary element on the page. */
	.saves {
		display: flex;
		align-items: center;
		gap: 12px;
		flex-wrap: wrap;
	}
	.save {
		height: 56px;
		min-width: 280px;
		padding: 0 14px 0 10px;
		display: flex;
		align-items: center;
		gap: 12px;
		border: 0;
		border-radius: 999px;
		background: var(--accent);
		color: var(--on-accent);
		font-size: 19px;
		font-weight: 650;
		letter-spacing: -0.015em;
		cursor: pointer;
	}
	.save:hover {
		filter: brightness(1.08);
	}
	.mark {
		width: 36px;
		height: 36px;
		border-radius: 50%;
		display: grid;
		place-items: center;
		background: color-mix(in srgb, var(--on-accent) 12%, transparent);
	}
	.mark svg {
		width: 18px;
		height: 18px;
	}
	.rewind {
		fill: currentColor;
	}
	.check {
		fill: none;
		stroke: currentColor;
		stroke-width: 2.6;
		stroke-linecap: round;
		stroke-linejoin: round;
		stroke-dasharray: 22;
		stroke-dashoffset: 22;
		animation: draw 260ms cubic-bezier(0.2, 0.8, 0.2, 1) 40ms forwards;
	}
	@keyframes draw {
		to {
			stroke-dashoffset: 0;
		}
	}
	.save-label {
		flex: 1;
		text-align: left;
	}
	.slot {
		height: 40px;
		padding: 0 10px 0 16px;
		display: flex;
		align-items: center;
		gap: 10px;
		border: 0;
		border-radius: 999px;
		background: var(--raised);
		box-shadow: inset 0 0 0 1px var(--line);
		color: var(--text);
		font-weight: 550;
		cursor: pointer;
	}
	.slot:hover {
		box-shadow: inset 0 0 0 1px var(--line-strong);
	}

	.tools {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		gap: 8px;
		max-width: 760px;
	}
	.tool {
		position: relative;
		padding: 12px;
		display: grid;
		grid-template-columns: 20px 1fr;
		grid-template-rows: auto auto;
		align-items: center;
		column-gap: 10px;
		row-gap: 8px;
		border: 0;
		border-radius: 8px;
		background: var(--raised);
		color: var(--text);
		text-align: left;
		cursor: pointer;
		transition:
			transform 160ms cubic-bezier(0.2, 0.8, 0.2, 1),
			box-shadow 160ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
	.tool:hover {
		transform: translateY(-2px);
		box-shadow:
			inset 0 0 0 1px var(--line-strong),
			0 6px 16px -8px var(--scrim-soft);
	}
	.tool svg {
		width: 20px;
		height: 20px;
		fill: none;
		stroke: var(--muted);
		stroke-width: 1.6;
		stroke-linejoin: round;
		stroke-linecap: round;
	}
	.tool:hover svg {
		stroke: var(--text);
	}
	.tool-label {
		font-weight: 550;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.tool :global(.keys) {
		grid-column: 2;
		justify-self: start;
	}
	.badge {
		position: absolute;
		right: 10px;
		top: 13px;
		color: var(--accent-2);
		font-size: 10.5px;
	}

	.markers {
		display: grid;
		gap: 8px;
		max-width: 760px;
	}
	.link {
		justify-self: start;
		padding: 0;
		border: 0;
		background: none;
		color: var(--muted);
		font-size: 13px;
		text-decoration: none;
		cursor: pointer;
	}
	.link:hover {
		color: var(--text);
	}

	.stats {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		gap: 8px;
		max-width: 760px;
	}
	.stat {
		display: flex;
		align-items: flex-end;
		justify-content: space-between;
		gap: 8px;
		padding: 10px 12px;
		border-radius: 8px;
		box-shadow: inset 0 0 0 1px var(--line);
		min-width: 0;
	}
	.stat-copy {
		display: grid;
		gap: 2px;
		min-width: 0;
	}
	.stat-n {
		font-size: 17px;
		font-weight: 500;
		color: var(--text);
		white-space: nowrap;
	}
	.stat-l {
		color: var(--muted);
		font-size: 12px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.spark {
		flex: none;
		width: 48px;
		height: 18px;
	}
	.spark rect {
		fill: var(--line-strong);
	}
	.spark rect.on {
		fill: var(--muted);
	}
	.ring {
		flex: none;
		width: 24px;
		height: 24px;
		transform: rotate(-90deg);
	}
	.ring circle {
		fill: none;
		stroke-width: 3;
	}
	.ring-bg {
		stroke: var(--line);
	}
	.ring-fg {
		stroke: var(--muted);
		stroke-linecap: round;
	}
	.ring.low .ring-fg {
		stroke: #f5a524;
	}

	.recent {
		display: grid;
		gap: 10px;
		min-width: 0;
	}
	.recent-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		max-width: 760px;
	}
	h2 {
		margin: 0;
		font-size: 13px;
		font-weight: 600;
		color: var(--muted);
	}
	.strip {
		list-style: none;
		margin: 0;
		padding: 0 0 6px;
		display: flex;
		gap: 8px;
		overflow-x: auto;
	}
	.clip {
		position: relative;
		display: block;
		width: 140px;
		aspect-ratio: 16 / 9;
		padding: 0;
		border: 0;
		border-radius: 6px;
		overflow: hidden;
		background: var(--sunken);
		box-shadow: inset 0 0 0 1px var(--line);
		cursor: pointer;
	}
	.clip img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}
	.clip-len {
		position: absolute;
		right: 4px;
		bottom: 4px;
		padding: 0 4px;
		border-radius: 4px;
		background: var(--scrim);
		color: var(--on-scrim);
		font-size: 10px;
	}
	.clip-over {
		position: absolute;
		inset: 0;
		padding: 8px;
		display: flex;
		flex-direction: column;
		justify-content: flex-end;
		gap: 2px;
		background: linear-gradient(to top, var(--scrim), transparent 80%);
		color: var(--on-scrim);
		font-size: 11px;
		text-align: left;
		opacity: 0;
		transition: opacity 140ms ease-out;
	}
	.clip-name {
		font-weight: 550;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
	.clip:hover .clip-over,
	.clip:focus-visible .clip-over {
		opacity: 1;
	}
	.clip:hover .clip-len,
	.clip:focus-visible .clip-len {
		opacity: 0;
	}
	.empty {
		margin: 0;
		color: var(--faint);
	}

	.statusline {
		margin-top: auto;
		padding-top: 12px;
		border-top: 1px solid var(--line);
		display: flex;
		flex-wrap: wrap;
		gap: 4px 20px;
		color: var(--faint);
		font-size: 11px;
	}
	.statusline .warn {
		color: #f5a524;
	}

	@media (max-width: 1000px) {
		.tools,
		.stats {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}
</style>
