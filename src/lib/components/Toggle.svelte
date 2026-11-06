<script lang="ts">
	let {
		checked,
		label,
		onchange,
		disabled = false
	}: { checked: boolean; label: string; onchange: (v: boolean) => void; disabled?: boolean } = $props();
</script>

<button
	role="switch"
	aria-checked={checked}
	aria-label={label}
	{disabled}
	class="switch"
	class:on={checked}
	onclick={() => onchange(!checked)}
>
	<span></span>
</button>

<style>
	.switch {
		width: 32px;
		height: 20px;
		padding: 2px;
		border: 0;
		border-radius: 12px;
		background: var(--line-strong);
		cursor: pointer;
		flex: none;
		transition: background-color 180ms var(--ease);
	}
	.switch:active {
		transform: none;
	}
	.switch span {
		display: block;
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: var(--ground);
		/* A small overshoot so the switch settles instead of stopping dead. */
		transition: transform 240ms cubic-bezier(0.34, 1.6, 0.64, 1);
	}
	.switch.on {
		background: var(--text);
	}
	.switch.on span {
		background: var(--ground);
		transform: translateX(12px);
	}
	.switch:disabled {
		opacity: 0.5;
		cursor: default;
	}
</style>
