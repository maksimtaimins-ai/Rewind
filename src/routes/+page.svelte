<script lang="ts">
	import { api, fileUrl, fmtDuration, type Clip, type DashboardWidgetId, type HotkeyAction } from '#lib/api.ts';
	import { app, inApp, refreshStorage, toast, updateSettings } from '#lib/stores/app.svelte.ts';
	import { onMount } from 'svelte';
	import StorageBar from '#lib/components/StorageBar.svelte';
	import BufferTimeline from '#lib/components/BufferTimeline.svelte';
	import Keycap from '#lib/components/Keycap.svelte';
	import MarkerList from '#lib/components/MarkerList.svelte';
	import Tour, { type Step } from '#lib/components/Tour.svelte';
	import EmptyState from '#lib/components/EmptyState.svelte';
	import Num from '#lib/components/Num.svelte';
	import Toggle from '#lib/components/Toggle.svelte';
	import { flip } from 'svelte/animate';
	import { ms, standard } from '#lib/motion.ts';
	import { gameOf } from '#lib/filter.ts';
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
	let bounce = $state(false);
	let seen: Clip | null = app.lastSaved;
	$effect(() => {
		const c = app.lastSaved;
		if (c && c !== seen) {
			seen = c;
			flashSecs = c.duration;
			flash++;
			bounce = true;
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
	const freeGb = $derived(s?.disk_free_gb ?? null);
	const sum = (a: number[]) => a.reduce((n, x) => n + x, 0);

	/** Bars for a sparkline, as SVG rects in a 60x20 box. */
	const bars = (data: number[]) => {
		const max = Math.max(1, ...data);
		const w = 60 / data.length;
		return data.map((v, i) => ({ x: i * w + w * 0.15, w: w * 0.7, h: v ? Math.max(2, (v / max) * 20) : 1, v }));
	};

	// Dashboard widgets: which are on and in what order lives in settings.
	const widgets = $derived(settings.dashboard);
	// Off widgets are hidden, except while customizing.
	const names: Record<DashboardWidgetId, string> = {
		save: 'Save button',
		buffer: 'Live buffer',
		recent: 'Recent clips',
		disk: 'Disk usage',
		counters: 'Clip counts',
		status: 'Capture status',
		playtime: 'Most clipped games'
	};
	// compact: shown in the smallest window. ess: from 400px. full: from 900px.
	const tiers: Record<DashboardWidgetId, string> = { save: 'compact', buffer: 'compact', counters: 'compact', recent: 'ess', status: 'ess', disk: 'full', playtime: 'full' };
	let editing = $state(false);
	const visible = $derived(widgets.filter((w) => w.enabled || editing));
	let dragId = $state<DashboardWidgetId | null>(null);
	/** Arrow buttons: move and save. */
	const move = (id: DashboardWidgetId, to: number) => {
		if (dragId) {
			// While dragging only the screen changes; the drop saves.
			const next = $state.snapshot(app.settings!);
			const i = next.dashboard.findIndex((w) => w.id === id);
			if (i < 0 || to < 0 || to >= next.dashboard.length || i === to) return;
			next.dashboard.splice(to, 0, next.dashboard.splice(i, 1)[0]);
			app.settings = next;
			return;
		}
		updateSettings((x) => {
			const i = x.dashboard.findIndex((w) => w.id === id);
			if (i < 0 || to < 0 || to >= x.dashboard.length || i === to) return;
			x.dashboard.splice(to, 0, x.dashboard.splice(i, 1)[0]);
		});
	};
	const setWidget = (id: DashboardWidgetId, on: boolean) =>
		updateSettings((x) => {
			const w = x.dashboard.find((w) => w.id === id);
			if (w && id !== 'save') w.enabled = on;
		});
	const counters = $derived([
		{ label: 'Clips today', n: sum(todayBins), bins: todayBins },
		{ label: 'This week', n: sum(weekBins), bins: weekBins },
		{ label: 'Total clips', n: app.clips.length, bins: monthBins }
	]);
	// Clips per game over the last 14 days, top four. Rewind doesn't track play time itself.
	const gameRows = $derived.by(() => {
		const from = today - 13 * DAY;
		const by = new Map<string, number[]>();
		for (const c of app.clips) {
			const i = Math.floor((c.modified - from) / DAY);
			if (i < 0 || i > 13) continue;
			const g = gameOf(c);
			const bins = by.get(g) ?? new Array(14).fill(0);
			bins[i]++;
			by.set(g, bins);
		}
		return [...by].map(([name, bins]) => ({ name, bins, total: sum(bins) })).sort((a, b) => b.total - a.total).slice(0, 4);
	});

	// First-run tour, once the setup wizard is done. Settings, Help replays it.
	const touring = $derived(settings.first_run_complete && !settings.tour_complete);
	const tourSteps = $derived<Step[]>([
		{
			target: 'save',
			title: `This saves the last ${capacity} seconds`,
			body: 'Click it, or press the hotkey mid-game. The clip lands in Clips a moment later.',
			keys: settings.hotkeys.save_clip,
			side: 'below'
		},
		{
			target: 'timeline',
			title: 'Your screen is always being buffered here',
			body: 'The ribbon is the sound of the last few seconds. Nothing is written to disk until you save.',
			side: 'below'
		},
		{ target: 'nav-clips', title: 'Your clips go here', body: 'Trim, tag, star and share them, or find one by game and date.', side: 'right' },
		{
			target: 'nav-settings',
			title: 'Customize everything in Settings',
			body: 'Hotkeys, buffer length, quality and sound. Replay this tour from Settings, under Help.',
			side: 'right'
		}
	]);
	let tourReady = $state(false);
	$effect(() => {
		// Let the page settle in before measuring what to point at.
		const t = setTimeout(() => (tourReady = true), 400);
		return () => clearTimeout(t);
	});

	const encoderName = $derived(s?.encoder?.replace(/ Encoder MFT$/, '').replace(/ Encoder$/, '') ?? null);
	// Measured once here, then refreshed when clips change; never polled.
	onMount(() => void refreshStorage());
</script>

<svelte:window onresize={() => innerWidth < 400 && (editing = false)} />

<div class="page" class:editing>
	{#if s && s.disk !== 'ok' && s.disk_free_gb != null}
		<p class="disk {s.disk}" role="status">
			{#if s.disk === 'full'}
				<strong>Only {s.disk_free_gb.toFixed(1)} GB free on the clips drive.</strong> Saving clips is paused until there's more than {settings.disk.block_gb} GB. Screenshots still work. <a href="/settings#storage">Storage</a>
			{:else}
				<strong>Only {s.disk_free_gb.toFixed(1)} GB free on the clips drive.</strong> Clips still save; below {settings.disk.block_gb} GB they'll stop. <a href="/settings#storage">Storage</a>
			{/if}
		</p>
	{/if}

	<div class="topbar">
		{#if settings.appearance.status_pills && !editing}
			<ul class="pills num" aria-label="Capture status">
				<li><span class="k">Encoder</span> {encoderName ?? '—'}</li>
				<li><span class="k">FPS</span> {live ? Math.round(s?.fps ?? 0) : `${settings.video.fps} target`}</li>
				<li><span class="k">Game</span> {s?.game ?? 'Desktop'}</li>
				<li><span class="k">Buffer</span> {s && s.buffer_secs ? Math.min(100, Math.round((s.buffered_secs / s.buffer_secs) * 100)) : 0}%</li>
			</ul>
		{/if}
		{#if editing}<p class="edit-hint">Drag widgets to reorder, or use the arrows. Switch off the ones you don't want.</p>{/if}
		<button class="customize" aria-pressed={editing} onclick={() => (editing = !editing)}>{editing ? 'Done' : 'Customize'}</button>
	</div>

	<div class="grid">
		{#each visible as w (w.id)}
			{@const i = widgets.findIndex((x) => x.id === w.id)}
				<section
					class="widget w-{w.id} tier-{tiers[w.id]}"
					class:off={!w.enabled}
					class:dragging={dragId === w.id}
					aria-label={names[w.id]}
					draggable={editing}
					ondragstart={() => (dragId = w.id)}
					ondragend={() => ((dragId = null), updateSettings(() => {}))}
					ondragover={(e) => editing && dragId && (e.preventDefault(), dragId !== w.id && move(dragId, i))}
					animate:flip={{ duration: ms(220), easing: standard }}
				>
					{#if editing}
						<div class="wbar">
							<span class="grip" aria-hidden="true">⠿</span>
							<span class="wname">{names[w.id]}</span>
							<button class="wb" aria-label="Move {names[w.id]} up" disabled={i === 0} onclick={() => move(w.id, i - 1)}>↑</button>
							<button class="wb" aria-label="Move {names[w.id]} down" disabled={i === widgets.length - 1} onclick={() => move(w.id, i + 1)}>↓</button>
							{#if w.id === 'save'}
								<span class="always">Always on</span>
							{:else}
								<Toggle checked={w.enabled} label="Show {names[w.id]}" onchange={(v) => setWidget(w.id, v)} />
							{/if}
						</div>
					{/if}

					{#if w.id === 'save'}
						<div class="saves">
							<button class="save" class:done={saved > 0} class:bounce={bounce} onanimationend={() => (bounce = false)} onclick={() => save()} data-tour="save">
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
							<div class="markers">
								<button class="link" aria-expanded={showMarkers} onclick={() => (showMarkers = !showMarkers)}>
									{showMarkers ? 'Hide markers' : `Show ${markers.length} ${markers.length === 1 ? 'marker' : 'markers'}`}
								</button>
								{#if showMarkers}
									<MarkerList {markers} maxSecs={capacity} onclip={(id, secs) => run(() => api.saveMarker(id, secs))} />
								{/if}
							</div>
						{/if}
					{:else if w.id === 'buffer'}
						<header class="hero-head">
							<h1>{s?.game ?? 'Desktop'}</h1>
							<p class="keep">
								{#if stateCopy}{stateCopy}{:else}<span class="keep-long">{keepCopy}</span><span class="held"><Num value={Math.floor(held)} suffix="s held" /></span>{/if}
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
					{:else if w.id === 'recent'}
						<div class="whead">
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
							<EmptyState compact>
								No clips yet.
								{#if settings.hotkeys.save_clip}Press <Keycap keys={settings.hotkeys.save_clip} /> to save your first.{:else}Click Save clip to save your first.{/if}
							</EmptyState>
						{/if}
					{:else if w.id === 'disk'}
						<div class="whead">
							<h2>Disk</h2>
							<a href="/settings#storage" class="link">Details</a>
						</div>
						<StorageBar data={app.storage} />
						<p class="free">
							{#if freeGb != null}<Num value={freeGb} digits={1} suffix=" GB" /> free on the clips drive{:else}Free space not measured yet{/if}
						</p>
					{:else if w.id === 'counters'}
						<div class="counters" aria-label="Clip counts">
							{#each counters as c (c.label)}
								<div class="stat">
									<div class="stat-copy"><span class="stat-n"><Num value={c.n} /></span><span class="stat-l">{c.label}</span></div>
									<svg class="spark" viewBox="0 0 60 20" aria-hidden="true">
										{#each bars(c.bins) as b, bi (bi)}<rect x={b.x} y={20 - b.h} width={b.w} height={b.h} class:on={b.v > 0} />{/each}
									</svg>
								</div>
							{/each}
						</div>
					{:else if w.id === 'status'}
						<div class="statusline num" aria-label="Capture details">
							<span>{encoderName ?? 'No encoder yet'}</span>
							<span>{#if live}<Num value={s?.fps ?? 0} suffix=" fps" />{:else}{settings.video.fps} fps target{/if}</span>
							{#if s?.resolution}<span>{s.resolution[0]}×{s.resolution[1]}</span>{/if}
							<span>{s?.game ? `Game: ${s.game}` : 'Desktop'}</span>
							{#if settings.performance.low_spec}<span>Low-spec</span>{/if}
							{#if live && s?.dropped}<span class="warn">{s.dropped} dropped</span>{/if}
						</div>
					{:else if w.id === 'playtime'}
						<div class="whead"><h2>Most clipped games</h2><span class="sub">14 days</span></div>
						{#if gameRows.length}
							<ul class="games">
								{#each gameRows as g (g.name)}
									<li>
										<span class="gname">{g.name}</span>
										<svg class="spark wide" viewBox="0 0 60 20" aria-hidden="true">
											{#each bars(g.bins) as b, bi (bi)}<rect x={b.x} y={20 - b.h} width={b.w} height={b.h} class:on={b.v > 0} />{/each}
										</svg>
										<span class="num gcount">{g.total}</span>
									</li>
								{/each}
							</ul>
						{:else}
							<EmptyState compact>Clips from the last two weeks show up here by game.</EmptyState>
						{/if}
					{/if}
				</section>
		{/each}
	</div>
</div>

{#if touring && tourReady}
	<Tour steps={tourSteps} ondone={() => updateSettings((x) => (x.tour_complete = true))} />
{/if}

<style>
	.page {
		min-height: 100%;
		box-sizing: border-box;
		padding: var(--s-5) var(--s-6) var(--s-4);
		display: flex;
		flex-direction: column;
		gap: var(--s-4);
		max-width: 1120px;
		transition: padding var(--dur) var(--ease);
	}
	.disk {
		margin: 0;
		padding: 10px 14px;
		border-radius: var(--r-2);
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

	.topbar {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: var(--s-4);
		min-height: 28px;
	}
	.edit-hint {
		margin: 0;
		margin-right: auto;
		color: var(--muted);
		font-size: 13px;
	}
	.pills {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		margin-right: auto;
	}
	.pills li {
		display: inline-flex;
		gap: 6px;
		align-items: center;
		height: 24px;
		padding: 0 10px;
		border-radius: 12px;
		background: var(--raised);
		box-shadow: inset 0 0 0 1px var(--border);
		font-size: 11px;
		white-space: nowrap;
	}
	.pills .k {
		color: var(--faint);
	}
	.customize {
		height: 28px;
		padding: 0 var(--s-3);
		border: 0;
		border-radius: var(--r-1);
		background: var(--raised);
		color: var(--muted);
		font-size: 13px;
		font-weight: 500;
		cursor: pointer;
	}
	.customize:hover,
	.customize[aria-pressed='true'] {
		color: var(--text);
	}

	/* Widgets: two columns when there is room, one when there isn't. */
	.grid {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		grid-auto-flow: dense;
		gap: var(--s-4);
		align-items: start;
	}
	.widget {
		min-width: 0;
		display: grid;
		gap: var(--s-3);
		grid-column: 1 / -1;
		transition:
			opacity var(--dur) var(--ease),
			transform var(--dur) var(--ease);
	}
	.w-disk,
	.w-counters,
	.w-playtime {
		grid-column: auto;
	}
	.w-disk,
	.w-playtime {
		padding: var(--s-4);
		border-radius: var(--r-3);
		background: var(--raised);
		box-shadow: inset 0 0 0 1px var(--border);
	}
	.editing .widget {
		padding: var(--s-3);
		border-radius: var(--r-3);
		box-shadow: inset 0 0 0 1px var(--border);
		background: color-mix(in srgb, var(--raised) 40%, transparent);
	}
	.editing .widget.off {
		opacity: 0.45;
	}
	.editing .widget.dragging {
		opacity: 0.6;
		transform: scale(0.99);
	}
	.wbar {
		display: flex;
		align-items: center;
		gap: var(--s-2);
		cursor: grab;
	}
	.grip {
		color: var(--faint);
	}
	.wname {
		flex: 1;
		font-weight: 550;
		font-size: 13px;
	}
	.wb {
		width: 24px;
		height: 24px;
		border: 0;
		border-radius: var(--r-1);
		background: var(--raised);
		cursor: pointer;
	}
	.wb:disabled {
		color: var(--faint);
		cursor: default;
	}
	.always {
		color: var(--faint);
		font-size: 12px;
	}
	.whead {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
	}
	.sub {
		color: var(--faint);
		font-size: 12px;
	}

	.hero-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--s-4);
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
		gap: var(--s-3);
		white-space: nowrap;
	}
	.held {
		color: var(--faint);
		font-family: var(--font-mono);
		font-size: 0.86em;
	}

	/* The one primary element on the page. */
	.saves {
		display: flex;
		align-items: center;
		gap: var(--s-3);
		flex-wrap: wrap;
	}
	.save {
		height: 56px;
		min-width: 280px;
		padding: 0 14px 0 10px;
		display: flex;
		align-items: center;
		gap: var(--s-3);
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
	/* A save, by hotkey or click, gives the button a little thump. */
	.save.bounce {
		animation: thump 320ms var(--ease);
	}
	@keyframes thump {
		0% {
			transform: scale(1);
		}
		30% {
			transform: scale(0.97);
		}
		65% {
			transform: scale(1.015);
		}
		100% {
			transform: scale(1);
		}
	}
	.save.done .mark {
		animation: settle 260ms var(--ease);
	}
	@keyframes settle {
		from {
			transform: scale(0.8);
		}
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
		padding: 0 10px 0 var(--s-4);
		display: flex;
		align-items: center;
		gap: 10px;
		border: 0;
		border-radius: 999px;
		background: var(--raised);
		box-shadow: inset 0 0 0 1px var(--border);
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
		gap: var(--s-2);
		max-width: 760px;
	}
	.tool {
		position: relative;
		padding: var(--s-3);
		display: grid;
		grid-template-columns: 20px 1fr;
		grid-template-rows: auto auto;
		align-items: center;
		column-gap: 10px;
		row-gap: var(--s-2);
		border: 0;
		border-radius: var(--r-2);
		background: var(--raised);
		color: var(--text);
		text-align: left;
		cursor: pointer;
		transition:
			transform 160ms cubic-bezier(0.2, 0.8, 0.2, 1),
			box-shadow 160ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
	.tool:active {
		transform: translateY(-1px) scale(0.97);
		transition-duration: 100ms;
	}
	.tool:hover:not(:active) {
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
		gap: var(--s-2);
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

	.counters {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: var(--s-2);
	}
	.stat {
		display: flex;
		align-items: flex-end;
		justify-content: space-between;
		gap: var(--s-2);
		padding: 10px var(--s-3);
		border-radius: var(--r-2);
		box-shadow: inset 0 0 0 1px var(--border);
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
	.free {
		margin: 0;
		color: var(--muted);
		font-size: 13px;
	}
	.games {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: var(--s-2);
	}
	.games li {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 96px 32px;
		align-items: center;
		gap: var(--s-3);
	}
	.gname {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.spark.wide {
		width: 96px;
		height: 20px;
	}
	.gcount {
		text-align: right;
		color: var(--muted);
	}

	.strip {
		list-style: none;
		margin: 0;
		padding: 0 0 6px;
		display: flex;
		gap: var(--s-2);
		overflow-x: auto;
	}
	.clip {
		position: relative;
		display: block;
		width: 140px;
		aspect-ratio: 16 / 9;
		padding: 0;
		border: 0;
		border-radius: var(--r-2);
		overflow: hidden;
		background: var(--sunken);
		box-shadow: inset 0 0 0 1px var(--border);
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
		border-radius: var(--r-1);
		background: var(--scrim);
		color: var(--on-scrim);
		font-size: 10px;
	}
	.clip-over {
		position: absolute;
		inset: 0;
		padding: var(--s-2);
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
	h2 {
		margin: 0;
		font-size: 13px;
		font-weight: 600;
		color: var(--muted);
	}

	.statusline {
		display: flex;
		flex-wrap: wrap;
		gap: 4px 20px;
		color: var(--faint);
		font-size: 11px;
	}
	.statusline .warn {
		color: #f5a524;
	}

	/* Narrower windows: fewer widgets, tighter spacing. Nothing restarts; it all follows the window. */
	@media (max-width: 1000px) {
		.tools {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}
	/* 900 and under: essentials only. */
	@media (max-width: 899px) {
		.grid {
			grid-template-columns: minmax(0, 1fr);
		}
		.widget {
			grid-column: auto;
		}
		.tier-full {
			display: none;
		}
		.editing .tier-full {
			display: grid;
		}
		.page {
			padding: var(--s-4) var(--s-4) var(--s-3);
		}
		.counters {
			max-width: 520px;
		}
	}
	/* Under 400: the save button, the buffer and one small stat. */
	@media (max-width: 399px) {
		.page {
			padding: var(--s-3);
			gap: var(--s-3);
		}
		.disk,
		.tier-ess,
		.tools,
		.markers,
		.keep-long,
		.topbar {
			display: none;
		}
		.save {
			min-width: 0;
			width: 100%;
			height: 48px;
			font-size: 17px;
		}
		.slot {
			display: none;
		}
		h1 {
			font-size: 17px;
		}
		.counters {
			grid-template-columns: 1fr;
		}
		.counters .stat:not(:first-child) {
			display: none;
		}
	}
</style>
