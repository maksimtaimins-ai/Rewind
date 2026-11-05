<script lang="ts">
	import '../app.css';
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { app, init } from '#lib/stores/app.svelte.ts';
	import TitleBar from '#lib/components/TitleBar.svelte';
	import Rail from '#lib/components/Rail.svelte';
	import SearchPalette from '#lib/components/SearchPalette.svelte';
	import { closeSearch, searchUi } from '#lib/stores/search.svelte.ts';
	import Toasts from '#lib/components/Toasts.svelte';
	import { apply as applyTheme, resolve as resolveTheme, SYSTEM, type Palette } from '#lib/themes.ts';
	import { fly } from 'svelte/transition';
	import { enter, ms, reducedMotion } from '#lib/motion.ts';

	let { children } = $props();

	// The compact HUD and the live keystrokes are
	// their own windows: just their content, no shell or toasts.
	const hud = ['/hud', '/keys'].some((p) => page.url.pathname.startsWith(p));

	onMount(() => {
		init({ hud });
	});

	// Windows light/dark, tracked live for the "Follow system" theme.
	let prefersLight = $state(false);
	onMount(() => {
		const mq = matchMedia('(prefers-color-scheme: light)');
		prefersLight = mq.matches;
		const on = (e: MediaQueryListEvent) => (prefersLight = e.matches);
		mq.addEventListener('change', on);
		return () => mq.removeEventListener('change', on);
	});

	// Crossfade the whole window when the theme actually changes (not on every settings save).
	let palette: Palette | null = null;
	$effect(() => {
		const next = resolveTheme(app.settings?.theme ?? SYSTEM, prefersLight);
		if (next === palette) return;
		const first = !palette;
		palette = next;
		if (first || reducedMotion() || !document.startViewTransition) applyTheme(next);
		else document.startViewTransition(() => applyTheme(next));
	});

	$effect(() => {
		if (!hud && app.ready && app.settings && !app.settings.first_run_complete && page.url.pathname !== '/wizard') {
			goto('/wizard');
		}
	});

	const bare = $derived(page.url.pathname.startsWith('/wizard'));
</script>

<svelte:window
	onkeydown={(e) => {
		if (hud || bare || !(e.ctrlKey || e.metaKey) || e.key.toLowerCase() !== 'k') return;
		e.preventDefault();
		if (searchUi.open) closeSearch();
		else searchUi.open = true;
	}}
/>

{#if hud}
	{#if app.ready}{@render children()}{/if}
{:else}
	<div class="shell">
		{#if !bare}
			<Rail />
		{/if}
		<div class="main">
			<TitleBar title={bare ? 'Rewind setup' : ''} search={!bare} />
			<main>
				{#if app.ready}
					{#key page.url.pathname}
						<div class="route" in:fly={{ y: 4, duration: ms(150), easing: enter }}>
							{@render children()}
						</div>
					{/key}
				{/if}
			</main>
		</div>
	</div>
	{#if !bare}<SearchPalette />{/if}
	<Toasts />
{/if}

<style>
	.shell {
		display: flex;
		height: 100vh;
	}
	.main {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}
	main {
		flex: 1;
		min-height: 0;
		overflow: auto;
	}
	.route {
		height: 100%;
	}
</style>
