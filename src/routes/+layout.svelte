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
	import { apply as applyTheme, resolve as resolveTheme, withAccent, SYSTEM, type Palette } from '#lib/themes.ts';
	import { applyFont, applyScale } from '#lib/appearance.ts';
	import { fmt } from '#lib/format.svelte.ts';
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
	let accentUsed: string | undefined;
	$effect(() => {
		const next = resolveTheme(app.settings?.theme ?? SYSTEM, prefersLight);
		const accent = app.settings?.appearance.accent ?? '';
		if (next === palette && accent === accentUsed) return;
		const first = !palette;
		const themeChanged = next !== palette;
		palette = next;
		accentUsed = accent;
		const eff = withAccent(next, accent);
		// An accent change alone is applied at once; only a new theme crossfades.
		if (first || !themeChanged || reducedMotion() || !document.startViewTransition) applyTheme(eff);
		else document.startViewTransition(() => applyTheme(eff));
	});

	// Time, date, duration and size styles for everything that shows one.
	$effect(() => {
		const a = app.settings?.appearance;
		fmt.time = a?.time_format ?? 'system';
		fmt.date = a?.date_format ?? 'friendly';
		fmt.duration = a?.duration_format ?? 'clock';
		fmt.size = a?.size_format ?? 'si';
	});

	// Typeface and UI scale. The HUD pill and live overlays keep their own size.
	$effect(() => applyFont(app.settings?.appearance.font ?? 'theme'));
	$effect(() => {
		if (!hud) applyScale(app.settings?.appearance.ui_scale ?? 100);
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
