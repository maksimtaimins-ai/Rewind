<script lang="ts">
	import { page } from '$app/state';
	import { app } from '#lib/stores/app.svelte.ts';
	import StatusLine from './StatusLine.svelte';

	const links = [
		{ href: '/', label: 'Live', tour: undefined, icon: 'M3 12l8-6v12zM12 12l8-6v12z' },
		{ href: '/library', label: 'Clips', tour: 'nav-clips', icon: 'M4 6h16v12H4zM10 9.5v5l4.5-2.5z' },
		{ href: '/settings', label: 'Settings', tour: 'nav-settings', icon: 'M5 7h14M5 12h14M5 17h14M9 5v4M15 10v4M8 15v4' }
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
				<a bind:this={els[i]} href={l.href} class:on={active(l.href)} aria-current={active(l.href) ? 'page' : undefined} aria-label={l.label} title={l.label} data-tour={l.tour}>
					<svg class="ico" viewBox="0 0 24 24" aria-hidden="true"><path d={l.icon} /></svg>
					<span class="lbl">{l.label}</span>
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
		overflow: hidden;
		transition: width var(--dur) var(--ease), padding var(--dur) var(--ease);
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
	.ico {
		display: none;
		width: 20px;
		height: 20px;
		fill: none;
		stroke: currentColor;
		stroke-width: 1.6;
		stroke-linejoin: round;
		stroke-linecap: round;
	}
	.lbl {
		flex: 1;
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
	/* Under 700px the rail keeps only icons, so the content gets the room. */
	@media (max-width: 699px) {
		.rail {
			width: 56px;
			padding-inline: var(--s-2);
		}
		.mark span,
		.lbl,
		.count,
		.local,
		.foot :global(.text) {
			display: none;
		}
		.mark {
			justify-content: center;
			padding-inline: 0;
		}
		ul a {
			justify-content: center;
			padding-inline: 0;
			height: 36px;
		}
		.ico {
			display: block;
		}
		.foot {
			padding: 0;
			justify-items: center;
		}
	}
</style>
