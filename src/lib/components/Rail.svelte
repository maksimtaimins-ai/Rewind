<script lang="ts">
	import { page } from '$app/state';
	import { app } from '#lib/stores/app.svelte.ts';
	import StatusLine from './StatusLine.svelte';

	const links = [
		{ href: '/', label: 'Live' },
		{ href: '/library', label: 'Clips' },
		{ href: '/settings', label: 'Settings' }
	];
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
		{#each links as l (l.href)}
			<li>
				<a href={l.href} class:on={active(l.href)} aria-current={active(l.href) ? 'page' : undefined}>
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
