<!-- Click, then press a key combination. Escape cancels, Backspace clears. -->
<script lang="ts">
	let { value, label, onchange }: { value: string; label: string; onchange: (v: string) => void } = $props();
	let listening = $state(false);

	const KEY_NAMES: Record<string, string> = {
		' ': 'Space',
		ArrowUp: 'Up',
		ArrowDown: 'Down',
		ArrowLeft: 'Left',
		ArrowRight: 'Right',
		PageUp: 'PageUp',
		PageDown: 'PageDown',
		Insert: 'Insert',
		Delete: 'Delete',
		Home: 'Home',
		End: 'End',
		Pause: 'Pause',
		ScrollLock: 'ScrollLock',
		PrintScreen: 'PrintScreen'
	};

	function keyName(e: KeyboardEvent): string | null {
		if (['Control', 'Shift', 'Alt', 'Meta'].includes(e.key)) return null;
		if (/^F\d{1,2}$/.test(e.key)) return e.key;
		if (e.code.startsWith('Numpad') && /\d$/.test(e.code)) return `Num${e.code.slice(-1)}`;
		if (KEY_NAMES[e.key]) return KEY_NAMES[e.key];
		if (e.code.startsWith('Key')) return e.code.slice(3);
		if (e.code.startsWith('Digit')) return e.code.slice(5);
		return e.key.length === 1 ? e.key : null;
	}

	function onkeydown(e: KeyboardEvent) {
		if (!listening) return;
		e.preventDefault();
		if (e.key === 'Escape') return (listening = false);
		if (e.key === 'Backspace' && !e.ctrlKey && !e.altKey) {
			onchange('');
			listening = false;
			return;
		}
		const k = keyName(e);
		if (!k) return;
		const parts = [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Win', k].filter(Boolean);
		onchange(parts.join('+'));
		listening = false;
	}
</script>

<button
	class="key"
	class:listening
	aria-label="{label}: {value || 'not set'}. Click to change."
	onclick={() => (listening = !listening)}
	{onkeydown}
	onblur={() => (listening = false)}
>
	{#if listening}
		Press keys…
	{:else if value}
		{#each value.split('+') as part, i (i)}
			<kbd>{part}</kbd>
		{/each}
	{:else}
		<span class="unset">Not set</span>
	{/if}
</button>

<style>
	.key {
		min-width: 128px;
		height: 32px;
		display: inline-flex;
		align-items: center;
		justify-content: flex-end;
		gap: 4px;
		padding: 0 8px;
		border: 0;
		border-radius: 4px;
		background: var(--sunken);
		color: var(--text);
		cursor: pointer;
	}
	.key.listening {
		box-shadow: inset 0 0 0 1px var(--text);
		color: var(--muted);
	}
	kbd {
		font-family: var(--font-mono);
		font-size: 12px;
		padding: 2px 8px;
		border-radius: 4px;
		background: var(--raised);
	}
	.unset {
		color: var(--faint);
	}
</style>
