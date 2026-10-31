<script lang="ts">
	import { app } from '#lib/stores/app.svelte.ts';
</script>

<div class="toasts" aria-live="polite">
	{#each app.toasts as t (t.id)}
		<div class="toast {t.level}" role={t.level === 'error' ? 'alert' : 'status'}>{t.message}</div>
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
		animation: rise 160ms ease-out;
	}
	.toast.error {
		background: var(--ground);
		color: var(--text);
		border: 1px solid var(--text);
	}
	.toast.warning {
		background: var(--raised);
		color: var(--text);
		border: 1px solid var(--line-strong);
	}
	@keyframes rise {
		from {
			transform: translateY(8px);
			opacity: 0;
		}
	}
</style>
