<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { fly, scale } from 'svelte/transition';
	import { flip } from 'svelte/animate';
	import { enter, exit, ms, standard } from '#lib/motion.ts';
	import TagChip from '#lib/components/TagChip.svelte';
	import TagEditor from '#lib/components/TagEditor.svelte';
	import { openPath } from '@tauri-apps/plugin-opener';
	import { api, fmtBytes, type Clip } from '#lib/api.ts';
	import { app, inApp, refreshClips, setFavorite, setTags, toast } from '#lib/stores/app.svelte.ts';
	import ClipTile from '#lib/components/ClipTile.svelte';
	import Keycap from '#lib/components/Keycap.svelte';
	import Segmented from '#lib/components/Segmented.svelte';
	import { allTags, applyFilters, emptyFilters, isFiltering, type DateRange, type LengthRange } from '#lib/filter.ts';
	import { defaultSessionName, groupSessions } from '#lib/sessions.ts';
	import { postToDiscord, sendToPeer } from '#lib/share.ts';
	import type { LanPeer } from '#lib/api.ts';

	let selecting = $state(false);
	let selected = $state<string[]>([]);
	let busy = $state(false);
	let f = $state(emptyFilters());

	const shown = $derived(applyFilters(app.clips, f));
	const tags = $derived(allTags(app.clips));
	// Favorites are pinned above the sessions instead of inside them.
	const favorites = $derived(shown.filter((c) => c.favorite));
	// Sessions come from every clip so their keys (and names) don't shift while filtering.
	const sessions = $derived(
		groupSessions(app.clips, app.settings?.library.session_gap_mins ?? 30)
			.map((s) => ({ ...s, shown: s.clips.filter((c) => !c.favorite && shown.includes(c)) }))
			.filter((s) => s.shown.length)
	);
	let collapsed = $state<Record<string, boolean>>({});
	let editing = $state<string | null>(null);
	const sessionName = (key: string, start: number) => app.meta.sessions[key] ?? defaultSessionName(start);

	async function renameSession(key: string, name: string) {
		editing = null;
		const current = app.meta.sessions[key] ?? '';
		if (name.trim() === current) return;
		app.meta.sessions = { ...app.meta.sessions, [key]: name.trim() };
		if (!name.trim()) delete app.meta.sessions[key];
		if (!inApp) return;
		try {
			await api.renameSession(key, name);
		} catch (e) {
			toast('error', String(e));
		}
	}

	const toggleTag = (t: string) => (f.tags = f.tags.includes(t) ? f.tags.filter((x) => x !== t) : [...f.tags, t]);

	// Arriving from a tag chip elsewhere (/library?tag=...) filters by it.
	onMount(() => {
		const t = page.url.searchParams.get('tag');
		if (t) f.tags = [t];
	});

	// Right-click menu on a clip, and the tag popover it can open.
	let menu = $state<{ clip: Clip; x: number; y: number } | null>(null);
	let tagging = $state<{ path: string; x: number; y: number } | null>(null);
	let popover = $state<HTMLDivElement>();
	const tagClip = $derived(tagging ? app.clips.find((c) => c.path === tagging!.path) : undefined);
	const place = (x: number, y: number, w: number, h: number) => ({
		x: Math.min(x, window.innerWidth - w - 8),
		y: Math.min(y, window.innerHeight - h - 8)
	});

	let peers = $state<LanPeer[]>([]);
	async function openMenu(c: Clip, e: MouseEvent) {
		tagging = null;
		if (inApp && app.settings?.sharing.lan_enabled) peers = await api.lanPeers().catch(() => []);
		const extra = (app.settings?.sharing.discord_webhooks.length ?? 0) + peers.length;
		menu = { clip: c, ...place(e.clientX, e.clientY, 200, 152 + extra * 36) };
	}

	function closeFloating(e: Event) {
		if (menu && !(e.target as HTMLElement).closest?.('.ctx')) menu = null;
		if (tagging && !popover?.contains(e.target as Node)) tagging = null;
	}
	const totalSize = $derived(app.clips.reduce((n, c) => n + c.size, 0));

	function toggle(c: Clip) {
		selecting = true;
		selected = selected.includes(c.path) ? selected.filter((p) => p !== c.path) : [...selected, c.path];
	}

	function stopSelecting() {
		selecting = false;
		selected = [];
	}

	async function remove() {
		if (!inApp || !selected.length) return;
		busy = true;
		try {
			await api.deleteClips(selected);
			toast('success', `Moved ${selected.length} ${selected.length === 1 ? 'clip' : 'clips'} to Recently deleted`);
			stopSelecting();
			await refreshClips();
		} catch (e) {
			toast('error', String(e));
		} finally {
			busy = false;
		}
	}

	async function removeOne(c: Clip) {
		if (!inApp) return;
		try {
			await api.deleteClips([c.path]);
			toast('success', 'Moved 1 clip to Recently deleted');
			await refreshClips();
		} catch (e) {
			toast('error', String(e));
		}
	}

	function editTags(c: Clip, e: MouseEvent) {
		menu = null;
		const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
		tagging = { path: c.path, ...place(r.left, r.bottom + 4, 260, 160) };
	}

	async function merge() {
		if (!inApp) return;
		busy = true;
		try {
			// Oldest first, so the merged clip plays in the order things happened.
			const order = app.clips.filter((c) => selected.includes(c.path)).reverse().map((c) => c.path);
			const out = await api.merge(order);
			toast('success', `Merged into "${out.name}"`);
			stopSelecting();
			await refreshClips();
		} catch (e) {
			toast('error', String(e));
		} finally {
			busy = false;
		}
	}

	async function openFolder() {
		if (inApp) await openPath(await api.clipsDir());
	}
</script>

<svelte:window
	onpointerdown={closeFloating}
	onkeydown={(e) => e.key === 'Escape' && ((menu = null), (tagging = null))}
/>

<div class="page">
	<header>
		<div>
			<h1>Clips</h1>
			<p class="sub"><span class="num">{app.clips.length}</span> clips, <span class="num">{fmtBytes(totalSize)}</span> on disk</p>
		</div>
		<div class="tools">
			{#if selecting}
				<span class="picked"><span class="num">{selected.length}</span> selected</span>
				<button class="secondary" disabled={busy || selected.length < 2 || selected.length > 5} onclick={merge} title="Join 2–5 clips end to end">
					Merge
				</button>
				<button class="secondary" disabled={busy || !selected.length} onclick={remove}>Delete</button>
				<button class="secondary" onclick={stopSelecting}>Done</button>
			{:else}
				<button class="secondary" onclick={() => (selecting = true)} disabled={!app.clips.length}>Select</button>
				<button class="secondary" onclick={openFolder}>Open folder</button>
				<a class="secondary" href="/library/deleted">Recently deleted</a>
			{/if}
		</div>
	</header>

	<div class="filters">
		<input type="search" placeholder="Search names and tags" bind:value={f.query} aria-label="Search clips" />
		<Segmented
			label="Date"
			value={f.date}
			options={[
				{ value: 'today' as DateRange, label: 'Today' },
				{ value: 'week' as DateRange, label: 'Week' },
				{ value: 'month' as DateRange, label: 'Month' },
				{ value: 'all' as DateRange, label: 'All' },
				{ value: 'custom' as DateRange, label: 'Custom' }
			]}
			onchange={(v) => (f.date = v)}
		/>
		{#if f.date === 'custom'}
			<span class="range">
				<input type="date" bind:value={f.from} aria-label="From date" />
				<span class="to">to</span>
				<input type="date" bind:value={f.to} aria-label="To date" />
			</span>
		{/if}
		<Segmented
			label="Length"
			value={f.length}
			options={[
				{ value: 'any' as LengthRange, label: 'Any length' },
				{ value: 'short' as LengthRange, label: '<10s' },
				{ value: 'medium' as LengthRange, label: '10–30s' },
				{ value: 'long' as LengthRange, label: '30s–1m' },
				{ value: 'xlong' as LengthRange, label: '1m+' }
			]}
			onchange={(v) => (f.length = v)}
		/>
		<button class="chip" class:on={f.favorites} aria-pressed={f.favorites} onclick={() => (f.favorites = !f.favorites)}>
			Favorites
		</button>
		{#each tags as t (t)}
			<TagChip tag={t} active={f.tags.includes(t)} onclick={() => toggleTag(t)} />
		{/each}
	</div>

	{#if shown.length}
		{#if favorites.length}
			<section class="session" aria-labelledby="fav-h">
				<div class="sess-head">
					<h2 id="fav-h" class="sess-title">Favorites</h2>
					<span class="count"><span class="num">{favorites.length}</span> {favorites.length === 1 ? 'clip' : 'clips'}</span>
				</div>
				<div class="grid">
					{#each favorites as c, i (c.path)}
						<div class="cell" in:fly|global={{ y: 6, duration: ms(220), delay: ms(Math.min(i, 16) * 30), easing: enter }} out:scale={{ start: 0.92, duration: ms(160), easing: exit }} animate:flip={{ duration: ms(220), easing: standard }}>
							<ClipTile
								clip={c}
								{selecting}
								selected={selected.includes(c.path)}
								onopen={() => goto(`/trim?path=${encodeURIComponent(c.path)}`)}
								onselect={() => toggle(c)}
								onfavorite={(v) => setFavorite(c.path, v)}
								ondelete={() => removeOne(c)}
								onedittags={(e) => editTags(c, e)}
								ontag={toggleTag}
								activeTags={f.tags}
								oncontextmenu={(e) => openMenu(c, e)}
							/>
						</div>
					{/each}
				</div>
			</section>
		{/if}
		{#each sessions as sess (sess.key)}
			{@const open = !collapsed[sess.key]}
			<section class="session" aria-label={sessionName(sess.key, sess.start)}>
				<div class="sess-head">
					<button class="caret" aria-expanded={open} aria-label="{open ? 'Collapse' : 'Expand'} session" onclick={() => (collapsed[sess.key] = open)}>
						<svg viewBox="0 0 10 10" aria-hidden="true"><path d="M3 2l4 3-4 3" /></svg>
					</button>
					{#if editing === sess.key}
						<!-- svelte-ignore a11y_autofocus -->
						<input
							class="sess-name"
							value={app.meta.sessions[sess.key] ?? ''}
							placeholder={defaultSessionName(sess.start)}
							autofocus
							onblur={(e) => renameSession(sess.key, e.currentTarget.value)}
							onkeydown={(e) => {
								if (e.key === 'Enter') e.currentTarget.blur();
								if (e.key === 'Escape') editing = null;
							}}
							aria-label="Session name"
						/>
					{:else}
						<button class="sess-title" ondblclick={() => (editing = sess.key)} onclick={() => (collapsed[sess.key] = open)}>
							{sessionName(sess.key, sess.start)}
						</button>
					{/if}
					<span class="count"><span class="num">{sess.shown.length}</span> {sess.shown.length === 1 ? 'clip' : 'clips'}</span>
					{#if editing !== sess.key}
						<button class="rename" onclick={() => (editing = sess.key)}>Rename</button>
					{/if}
				</div>
				{#if open}
					<div class="grid">
						{#each sess.shown as c, i (c.path)}
							<div class="cell" in:fly|global={{ y: 6, duration: ms(220), delay: ms(Math.min(i, 16) * 30), easing: enter }} out:scale={{ start: 0.92, duration: ms(160), easing: exit }} animate:flip={{ duration: ms(220), easing: standard }}>
								<ClipTile
									clip={c}
									{selecting}
									selected={selected.includes(c.path)}
									onopen={() => goto(`/trim?path=${encodeURIComponent(c.path)}`)}
									onselect={() => toggle(c)}
									onfavorite={(v) => setFavorite(c.path, v)}
									ondelete={() => removeOne(c)}
									onedittags={(e) => editTags(c, e)}
									ontag={toggleTag}
									activeTags={f.tags}
									oncontextmenu={(e) => openMenu(c, e)}
								/>
							</div>
						{/each}
					</div>
				{/if}
			</section>
		{/each}
	{:else if app.clips.length}
		<p class="empty">
			No clips match these filters.
			{#if isFiltering(f)}<button class="link" onclick={() => (f = emptyFilters())}>Clear filters</button>{/if}
		</p>
	{:else}
		<div class="empty">
			<p>No clips yet.</p>
			<p>
				While a game is running, press
				{#if app.settings?.hotkeys.save_clip}<Keycap keys={app.settings.hotkeys.save_clip} />{:else}your save hotkey{/if}
				and the last {app.settings?.buffer.seconds} seconds land here.
			</p>
		</div>
	{/if}
</div>

{#if menu}
	{@const m = menu}
	<div class="ctx" role="menu" style="left: {m.x}px; top: {m.y}px">
		<button
			role="menuitem"
			onclick={() => {
				tagging = { path: m.clip.path, ...place(m.x, m.y, 260, 160) };
				menu = null;
			}}>Add tag</button
		>
		<button
			role="menuitem"
			onclick={() => {
				setFavorite(m.clip.path, !m.clip.favorite);
				menu = null;
			}}>{m.clip.favorite ? 'Remove from favorites' : 'Add to favorites'}</button
		>
		<button role="menuitem" onclick={() => (menu = null, goto(`/trim?path=${encodeURIComponent(m.clip.path)}`))}>Open</button>
		{#each app.settings?.sharing.discord_webhooks ?? [] as w, i (i)}
			<button role="menuitem" onclick={() => ((menu = null), postToDiscord(m.clip, i))}>Post to {w.name || 'Discord'}</button>
		{/each}
		{#each peers as p (p.id)}
			<button role="menuitem" onclick={() => ((menu = null), sendToPeer(m.clip, p))}>Send to {p.name}</button>
		{/each}
		<button role="menuitem" onclick={() => ((menu = null), removeOne(m.clip))}>Delete</button>
	</div>
{/if}

{#if tagging && tagClip}
	{@const c = tagClip}
	<div class="pop" bind:this={popover} style="left: {tagging.x}px; top: {tagging.y}px" role="dialog" aria-label="Tags for {c.name}">
		{#if c.tags.length}
			<div class="pop-tags">
				{#each c.tags as t (t)}
					<TagChip tag={t} onremove={() => setTags(c.path, c.tags.filter((x) => x !== t))} />
				{/each}
			</div>
		{/if}
		<TagEditor
			tags={c.tags}
			autofocus
			onadd={(tag, color) => setTags(c.path, [...c.tags, tag], color ? { tag, color } : undefined)}
		/>
	</div>
{/if}

<style>
	.ctx,
	.pop {
		position: fixed;
		z-index: 40;
		border-radius: 6px;
		transform-origin: top left;
		animation: pop-in 140ms ease-out;
		background: var(--raised);
		box-shadow: 0 8px 24px var(--scrim-soft);
	}
	.ctx {
		width: 200px;
		padding: 4px;
		display: grid;
	}
	.ctx button {
		height: 32px;
		padding: 0 12px;
		border: 0;
		border-radius: 4px;
		background: none;
		text-align: left;
		cursor: pointer;
	}
	.ctx button:hover,
	.ctx button:focus-visible {
		background: var(--line);
	}
	.pop {
		width: 260px;
		padding: 12px;
		display: grid;
		gap: 12px;
	}
	.pop-tags {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
	}
	.page {
		padding: 32px;
	}
	header {
		display: flex;
		justify-content: space-between;
		align-items: flex-end;
		gap: 24px;
		margin-bottom: 24px;
	}
	h1 {
		margin: 0;
		font-size: 24px;
		font-weight: 650;
		letter-spacing: -0.02em;
	}
	.sub {
		margin: 4px 0 0;
		color: var(--muted);
	}
	.tools {
		display: flex;
		gap: 8px;
		align-items: center;
	}
	.picked {
		color: var(--muted);
		margin-right: 8px;
	}
	.filters {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
		margin-bottom: 32px;
	}
	input[type='search'],
	input[type='date'] {
		height: 32px;
		padding: 0 12px;
		border-radius: 4px;
		border: 0;
		background: var(--sunken);
		outline: none;
		user-select: text;
	}
	input[type='search'] {
		width: 240px;
	}
	input[type='date'] {
		height: 28px;
		color-scheme: inherit;
	}
	.range {
		display: flex;
		align-items: center;
		gap: 4px;
	}
	.to {
		color: var(--faint);
	}
	.chip {
		height: 28px;
		padding: 0 12px;
		border: 0;
		border-radius: 4px;
		background: var(--sunken);
		color: var(--muted);
		cursor: pointer;
	}
	.chip:hover {
		color: var(--text);
	}
	.chip.on {
		background: var(--line-strong);
		color: var(--text);
	}
	.link {
		border: 0;
		background: none;
		padding: 0;
		color: var(--text);
		text-decoration: underline;
		cursor: pointer;
	}
	a.secondary {
		display: inline-flex;
		align-items: center;
		text-decoration: none;
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
	.session {
		margin-bottom: 32px;
	}
	.sess-head {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-bottom: 12px;
	}
	.caret {
		width: 24px;
		height: 24px;
		display: grid;
		place-items: center;
		border: 0;
		border-radius: 4px;
		background: transparent;
		color: var(--muted);
		cursor: pointer;
	}
	.caret svg {
		width: 10px;
		height: 10px;
		fill: none;
		stroke: currentColor;
		stroke-width: 1.5;
		transition: transform 120ms ease-out;
	}
	.caret[aria-expanded='true'] svg {
		transform: rotate(90deg);
	}
	.sess-title {
		margin: 0;
		border: 0;
		background: none;
		padding: 0;
		color: var(--muted);
		font-size: 13px;
		font-weight: 500;
		cursor: pointer;
	}
	.sess-title:hover {
		color: var(--text);
	}
	.sess-name {
		height: 28px;
		width: 280px;
		padding: 0 8px;
		font-size: 13px;
		font-weight: 500;
		border: 0;
		border-radius: 4px;
		background: var(--sunken);
		outline: none;
		user-select: text;
	}
	.count {
		color: var(--faint);
		font-size: 12px;
	}
	.rename {
		margin-left: auto;
		border: 0;
		background: none;
		color: var(--faint);
		font-size: 13px;
		cursor: pointer;
	}
	.rename {
		opacity: 0;
	}
	.rename:hover,
	.rename:focus-visible,
	.sess-head:hover .rename {
		opacity: 1;
		color: var(--muted);
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
		gap: 24px;
	}
	.empty {
		color: var(--muted);
		max-width: 56ch;
	}
	.empty p {
		margin: 0 0 8px;
	}
</style>
