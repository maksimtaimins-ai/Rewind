<script lang="ts">
	let {
		checked,
		label,
		onchange
	}: { checked: boolean; label: string; onchange: (v: boolean) => void } = $props();
</script>

<button
	role="switch"
	aria-checked={checked}
	aria-label={label}
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
		border-radius: 10px;
		background: var(--line-strong);
		cursor: pointer;
		flex: none;
	}
	.switch span {
		display: block;
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: var(--ground);
		transition: transform 120ms ease-out;
	}
	.switch.on {
		background: var(--text);
	}
	.switch.on span {
		background: var(--ground);
		transform: translateX(12px);
	}
</style>
