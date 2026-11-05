<script lang="ts">
	import { page } from '$app/state';
	import { app } from '#lib/stores/app.svelte.ts';
	import StatusLine from './StatusLine.svelte';

	const links = [
		{ href: '/', label: 'Live', tour: undefined },
		{ href: '/library', label: 'Clips', tour: 'nav-clips' },
		{ href: '/settings', label: 'Settings', tour: 'nav-settings' }
	];
	let els = $state<HTMLElement[]>([]);
	// The accent bar rides to whichever link is current.
	let bar = $state({ y: 0, h: 0, show: false });
	$effect(() => {
		const i = links.findIndex((l) => active(l.href));
		const el = els[i];
		bar = el ? { y: el.offsetTop, h: el.offsetHeight, show: true } : { ...bar, show: false };
	});
	const active = (href: string) =>
		href === '/' ? page.url.pathname === '/' : page.url.pathname.startsWith(href) || (href === '/library' && page.url.pathname.startsWith('/trim'));
</script>

<nav class="rail" aria-label="Main">
	<a href="/" class="mark" aria-label="Rewind home">
		<svg viewBox="0 0 24 24" aria-hidden="true">
			<path d="M3 12l8-6v12zM12 12l8-6v12z" />
		</svg>
		<span>Rewind</span>
	</a>
	<ul>
		<span class="bar" class:show={bar.show} style:transform="translateY({bar.y}px)" style:height="{bar.h}px" aria-hidden="true"></span>
		{#each links as l, i (l.href)}
			<li>
				<a bind:this={els[i]} href={l.href} class:on={active(l.href)} aria-current={active(l.href) ? 'page' : undefined} data-tour={l.tour}>
					{l.label}
					{#if l.href === '/library' && app.clips.length}
						<span class="count num">{app.clips.length}</span>
					{/if}
				</a>
			</li>
		{/each}
	</ul>
	<div class="foot">
		<StatusLine status={app.status} compact />
		{#if app.transfer}
			{@const t = app.transfer}
			<p class="local num">{t.direction === 'in' ? 'Receiving from' : 'Sending to'} {t.peer}, {Math.round((t.done / t.total) * 100)}%</p>
		{:else}
			<p class="local">No cloud, no account.</p>
		{/if}
	</div>
</nav>

<style>
	.rail {
		width: 200px;
		flex: none;
		display: flex;
		flex-direction: column;
		padding: 8px 12px 16px;
		background: var(--raised);
	}
	.mark {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px;
		margin-bottom: 16px;
		color: var(--text);
		text-decoration: none;
		font-weight: 650;
		font-size: 16px;
		letter-spacing: -0.01em;
	}
	.mark svg {
		width: 20px;
		height: 20px;
		fill: currentColor;
	}
	ul {
		position: relative;
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 4px;
	}
	ul a {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 8px 12px;
		border-radius: 4px;
		color: var(--muted);
		text-decoration: none;
		font-weight: 500;
	}
	ul a:hover {
		color: var(--text);
	}
	.bar {
		position: absolute;
		left: 0;
		top: 0;
		width: 3px;
		border-radius: 0 3px 3px 0;
		background: var(--accent);
		opacity: 0;
		transition:
			transform 220ms var(--ease),
			height 220ms var(--ease),
			opacity 180ms var(--ease);
	}
	.bar.show {
		opacity: 1;
	}
	ul a.on {
		color: var(--text);
		background: var(--ground);
	}
	.count {
		color: var(--faint);
	}
	.foot {
		margin-top: auto;
		display: grid;
		gap: 8px;
		padding: 0 8px;
		font-size: 12px;
	}
	.local {
		margin: 0;
		color: var(--faint);
	}
</style>
