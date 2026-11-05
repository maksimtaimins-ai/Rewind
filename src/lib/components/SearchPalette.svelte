<!--
  Command bar (Ctrl+K). Searches clips, settings and help in one list; arrows
  move, Enter opens, Esc closes. Results are grouped by where they live.
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import { tick } from 'svelte';
	import { fade, scale } from 'svelte/transition';
	import { app } from '#lib/stores/app.svelte.ts';
	import { closeSearch, searchUi } from '#lib/stores/search.svelte.ts';
	import { fileUrl } from '#lib/api.ts';
	import { inApp } from '#lib/stores/app.svelte.ts';
	import { clipItems, helpItems, search, settingItems, suggestions, type Group, type Item } from '#lib/search.ts';
	import { enter, exit, ms } from '#lib/motion.ts';

	let query = $state('');
	let active = $state(0);
	let input = $state<HTMLInputElement>();
	let list = $state<HTMLElement>();

	// Built when the bar opens, so typing only ranks.
	const items = $derived(
		searchUi.open ? [...clipItems(app.clips), ...settingItems(app.settings?.pro_mode ?? false), ...helpItems(app.settings)] : []
	);
	const results = $derived(query.trim() ? search(items, query) : suggestions(items));
	const groups = $derived(
		(['Clips', 'Settings', 'Help'] as Group[])
			.map((g) => ({ name: g, hits: results.filter((r) => r.group === g) }))
			.filter((g) => g.hits.length)
	);
	// Flat order the arrow keys walk, matching what is drawn.
	const flat = $derived(groups.flatMap((g) => g.hits));

	$effect(() => {
		if (searchUi.open) {
			query = '';
			active = 0;
			tick().then(() => input?.focus());
		}
	});
	$effect(() => {
		void query;
		active = 0;
	});

	async function go(it: Item) {
		closeSearch();
		await goto(it.to);
		// Same-page jumps (already in Settings) don't fire hashchange on their own.
		if (it.to.includes('#')) window.dispatchEvent(new HashChangeEvent('hashchange'));
	}

	function onkey(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			closeSearch();
		} else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
			e.preventDefault();
			if (!flat.length) return;
			active = (active + (e.key === 'ArrowDown' ? 1 : -1) + flat.length) % flat.length;
			tick().then(() => list?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' }));
		} else if (e.key === 'Enter' && flat[active]) {
			e.preventDefault();
			go(flat[active]);
		} else if (e.key === 'Tab') {
			// Keep focus in the bar.
			e.preventDefault();
		}
	}

	/** The title split around the matched words, so they can be marked. */
	function parts(title: string): { t: string; hit: boolean }[] {
		const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
		if (!words.length) return [{ t: title, hit: false }];
		const re = new RegExp(`(${words.map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|')})`, 'ig');
		return title.split(re).filter(Boolean).map((t) => ({ t, hit: words.includes(t.toLowerCase()) }));
	}
	const offset = (g: number) => groups.slice(0, g).reduce((n, x) => n + x.hits.length, 0);
</script>

{#if searchUi.open}
	<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
	<div class="veil" transition:fade={{ duration: ms(120) }} onpointerdown={(e) => e.target === e.currentTarget && closeSearch()}>
		<div class="bar" role="dialog" aria-modal="true" aria-label="Search" in:scale={{ start: 0.97, duration: ms(160), easing: enter }} out:scale={{ start: 0.98, duration: ms(110), easing: exit }}>
			<div class="field">
				<svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="7" cy="7" r="4.5" /><path d="M10.5 10.5L14 14" /></svg>
				<input
					bind:this={input}
					bind:value={query}
					onkeydown={onkey}
					placeholder="Search clips, settings and help"
					spellcheck="false"
					autocomplete="off"
					role="combobox"
					aria-expanded="true"
					aria-controls="search-results"
					aria-activedescendant={flat[active] ? 'hit-' + active : undefined}
					aria-label="Search"
				/>
				<kbd>Esc</kbd>
			</div>
			<div class="results" id="search-results" role="listbox" bind:this={list}>
				{#each groups as g, gi (g.name)}
					<div class="group" role="presentation">{query.trim() ? g.name : g.name === 'Clips' ? 'Latest clips' : g.name === 'Settings' ? 'Common settings' : 'Help'}</div>
					{#each g.hits as it, i (it.id)}
						{@const n = offset(gi) + i}
						<!-- svelte-ignore a11y_click_events_have_key_events -->
						<div id="hit-{n}" class="hit" class:on={n === active} role="option" aria-selected={n === active} tabindex="-1" onpointermove={() => (active = n)} onclick={() => go(it)}>
							{#if it.group === 'Clips'}
								<span class="thumb">{#if it.thumb && inApp}<img src={fileUrl(it.thumb)} alt="" loading="lazy" />{/if}</span>
							{:else}
								<span class="glyph" aria-hidden="true">
									{#if it.group === 'Settings'}
										<svg viewBox="0 0 16 16"><path d="M2 5h12M2 11h12" /><circle cx="6" cy="5" r="1.6" /><circle cx="10.5" cy="11" r="1.6" /></svg>
									{:else}
										<svg viewBox="0 0 16 16"><circle cx="8" cy="8" r="5.5" /><path d="M6.4 6.3a1.7 1.7 0 1 1 2.4 1.5c-.5.3-.8.6-.8 1.2M8 11.4v.1" /></svg>
									{/if}
								</span>
							{/if}
							<span class="text">
								<span class="title">{#each parts(it.title) as p}{#if p.hit}<mark>{p.t}</mark>{:else}{p.t}{/if}{/each}</span>
								{#if it.sub}<span class="sub">{it.sub}</span>{/if}
							</span>
							{#if n === active}<kbd class="enter">↵</kbd>{/if}
						</div>
					{/each}
				{:else}
					<p class="none">Nothing matches “{query.trim()}”. Try a clip name, a game, a tag or a setting like “bitrate”.</p>
				{/each}
			</div>
			<div class="foot"><span><kbd>↑</kbd><kbd>↓</kbd> move</span><span><kbd>↵</kbd> open</span><span><kbd>Esc</kbd> close</span></div>
		</div>
	</div>
{/if}

<style>
	.veil {
		position: fixed;
		inset: 0;
		z-index: 60;
		display: flex;
		justify-content: center;
		align-items: flex-start;
		padding-top: 14vh;
		background: var(--scrim-soft);
	}
	.bar {
		width: min(600px, calc(100vw - 32px));
		max-height: 70vh;
		display: flex;
		flex-direction: column;
		border-radius: 12px;
		background: var(--raised);
		box-shadow: 0 0 0 1px var(--line-strong), 0 24px 64px var(--scrim);
		overflow: hidden;
		transform-origin: top center;
	}
	.field {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 0 16px;
		height: 52px;
		border-bottom: 1px solid var(--line);
	}
	.field svg {
		width: 16px;
		height: 16px;
		flex: none;
		fill: none;
		stroke: var(--muted);
		stroke-width: 1.5;
		stroke-linecap: round;
	}
	input {
		flex: 1;
		min-width: 0;
		height: 100%;
		border: 0;
		outline: none;
		background: none;
		font-size: 15px;
		color: var(--text);
		user-select: text;
	}
	input::placeholder {
		color: var(--faint);
	}
	kbd {
		display: inline-grid;
		place-items: center;
		min-width: 20px;
		height: 20px;
		padding: 0 6px;
		border-radius: 4px;
		background: var(--sunken);
		color: var(--muted);
		font: 500 11px var(--font-sans);
	}
	.results {
		overflow-y: auto;
		padding: 4px 8px 8px;
		min-height: 80px;
	}
	.group {
		padding: 12px 8px 4px;
		color: var(--faint);
		font-size: 12px;
		font-weight: 500;
	}
	.hit {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 6px 8px;
		min-height: 44px;
		border-radius: 8px;
		cursor: pointer;
	}
	.hit.on {
		background: var(--line);
	}
	.thumb,
	.glyph {
		flex: none;
		width: 56px;
		height: 32px;
		display: grid;
		place-items: center;
		border-radius: 4px;
		background: var(--sunken);
		overflow: hidden;
	}
	.glyph {
		width: 32px;
		border-radius: 8px;
	}
	.thumb img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
	.glyph svg {
		width: 16px;
		height: 16px;
		fill: none;
		stroke: var(--muted);
		stroke-width: 1.4;
		stroke-linecap: round;
	}
	.hit.on .glyph svg {
		stroke: var(--text);
	}
	.text {
		display: grid;
		min-width: 0;
		flex: 1;
	}
	.title {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.title :global(mark) {
		background: none;
		color: var(--text);
		font-weight: 650;
		text-decoration: underline;
		text-decoration-color: var(--line-strong);
		text-underline-offset: 3px;
	}
	.sub {
		color: var(--muted);
		font-size: 12px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.enter {
		flex: none;
	}
	.none {
		margin: 0;
		padding: 24px 16px;
		color: var(--muted);
	}
	.foot {
		display: flex;
		gap: 16px;
		padding: 8px 16px;
		border-top: 1px solid var(--line);
		color: var(--faint);
		font-size: 12px;
	}
	.foot span {
		display: inline-flex;
		gap: 4px;
		align-items: center;
	}
</style>
