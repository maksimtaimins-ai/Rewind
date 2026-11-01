<script lang="ts">
	import '../app.css';
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { app, init } from '#lib/stores/app.svelte.ts';
	import TitleBar from '#lib/components/TitleBar.svelte';
	import Rail from '#lib/components/Rail.svelte';
	import Toasts from '#lib/components/Toasts.svelte';
	import { apply as applyTheme, resolve as resolveTheme, SYSTEM } from '#lib/themes.ts';

	let { children } = $props();

	// The compact HUD is its own window: just the pill, no shell or toasts.
	const hud = page.url.pathname.startsWith('/hud');

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

	$effect(() => {
		applyTheme(resolveTheme(app.settings?.theme ?? SYSTEM, prefersLight));
	});

	$effect(() => {
		if (!hud && app.ready && app.settings && !app.settings.first_run_complete && page.url.pathname !== '/wizard') {
			goto('/wizard');
		}
	});

	const bare = $derived(page.url.pathname.startsWith('/wizard'));
</script>

{#if hud}
	{#if app.ready}{@render children()}{/if}
{:else}
	<div class="shell">
		{#if !bare}
			<Rail />
		{/if}
		<div class="main">
			<TitleBar title={bare ? 'Rewind setup' : ''} />
			<main>
				{#if app.ready}
					{@render children()}
				{/if}
			</main>
		</div>
	</div>
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
</style>
