<script lang="ts">
	import { app } from '#lib/stores/app.svelte.ts';
	import { fly } from 'svelte/transition';
	import { flip } from 'svelte/animate';
	import { enter, exit, ms, standard } from '#lib/motion.ts';

	const pos = $derived((app.settings?.notifications.position ?? 'bottom_right').replace('_', '-'));
	// New toasts come in from the screen edge they sit on.
	const rise = $derived(pos.startsWith('top') ? -16 : 16);

	// Post-roll countdown: status arrives every 500 ms, so tick locally between updates.
	const post = $derived(app.status?.post_roll ?? null);
	let deadline = 0;
	let left = $state(0);
	$effect(() => {
		if (!post) return;
		deadline = performance.now() + post.remaining * 1000;
		left = post.remaining;
	});
	$effect(() => {
		if (!post) return;
		const id = setInterval(() => (left = Math.max(0, (deadline - performance.now()) / 1000)), 100);
		return () => clearInterval(id);
	});
</script>

<div class="toasts {pos}" aria-live="polite">
	{#if post}
		<div class="toast post" role="status" in:fly={{ y: rise, duration: ms(200), easing: enter }} out:fly={{ y: rise, duration: ms(150), easing: exit }}>
			<span class="rec" aria-hidden="true"></span>
			<span>Recording +{post.seconds}s…</span>
			<span class="num left">{Math.ceil(left)}s</span>
			{#if post.queued}<span class="queued">{post.queued} queued</span>{/if}
		</div>
	{/if}
	{#each app.toasts as t, i (t.id)}
		<div
			class="toast {t.level}"
			class:older={i < app.toasts.length - 1}
			class:oldest={i === 0 && app.toasts.length > 2}
			role={t.level === 'error' ? 'alert' : 'status'}
			in:fly={{ y: rise, duration: ms(200), easing: enter }}
			out:fly={{ y: -rise * 0.9, duration: ms(180), easing: exit }}
			animate:flip={{ duration: ms(200), easing: standard }}
		>
			{t.message}
		</div>
	{/each}
</div>

<style>
	.toasts {
		position: fixed;
		display: grid;
		gap: 8px;
		z-index: 50;
		max-width: 360px;
	}
	.toasts.bottom-right,
	.toasts.top-right {
		right: 16px;
	}
	.toasts.bottom-left,
	.toasts.top-left {
		left: 16px;
	}
	.toasts.top-center,
	.toasts.bottom-center {
		left: 50%;
		translate: -50% 0;
		width: max-content;
	}
	.toasts.bottom-right,
	.toasts.bottom-left,
	.toasts.bottom-center {
		bottom: 16px;
	}
	/* Below the title bar. */
	.toasts.top-right,
	.toasts.top-left,
	.toasts.top-center {
		top: 44px;
	}
	.toast {
		padding: 12px 16px;
		border-radius: 4px;
		background: var(--text);
		color: var(--ground);
		font-weight: 500;
	}
	/* Older toasts recede so the newest reads first; they climb as it arrives. */
	.toast.older {
		opacity: 0.82;
	}
	.toast.oldest {
		opacity: 0.6;
	}
	.toast {
		transition: opacity 180ms var(--ease);
	}
	.toast.error {
		background: var(--danger);
		color: var(--on-danger);
	}
	.toast.warning {
		background: var(--raised);
		color: var(--text);
		box-shadow: 0 8px 24px var(--scrim-soft);
	}
	.toast.post {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.post .rec {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--signal);
	}
	.post .left {
		margin-left: auto;
		font-family: var(--font-mono);
	}
	.post .queued {
		opacity: 0.7;
		font-weight: 400;
	}
</style>
