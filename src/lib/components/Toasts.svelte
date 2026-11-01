<script lang="ts">
	import { app } from '#lib/stores/app.svelte.ts';
	import { fly } from 'svelte/transition';
	import { flip } from 'svelte/animate';
	import { enter, exit, ms, standard } from '#lib/motion.ts';

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

<div class="toasts" aria-live="polite">
	{#if post}
		<div class="toast post" role="status" in:fly={{ y: 16, duration: ms(200), easing: enter }} out:fly={{ y: 16, duration: ms(150), easing: exit }}>
			<span class="rec" aria-hidden="true"></span>
			<span>Recording +{post.seconds}s…</span>
			<span class="num left">{Math.ceil(left)}s</span>
			{#if post.queued}<span class="queued">{post.queued} queued</span>{/if}
		</div>
	{/if}
	{#each app.toasts as t (t.id)}
		<div
			class="toast {t.level}"
			role={t.level === 'error' ? 'alert' : 'status'}
			in:fly={{ y: 16, duration: ms(200), easing: enter }}
			out:fly={{ y: 16, duration: ms(150), easing: exit }}
			animate:flip={{ duration: ms(200), easing: standard }}
		>
			{t.message}
		</div>
	{/each}
</div>

<style>
	.toasts {
		position: fixed;
		right: 16px;
		bottom: 16px;
		display: grid;
		gap: 8px;
		z-index: 50;
		max-width: 360px;
	}
	.toast {
		padding: 12px 16px;
		border-radius: 4px;
		background: var(--text);
		color: var(--ground);
		font-weight: 500;
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
