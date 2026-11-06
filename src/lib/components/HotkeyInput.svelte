<!--
  Click, then press any key combination: modifiers are optional. Escape cancels; Backspace or
  "None" clears the binding, which turns that function's hotkey off. A combo another Rewind hotkey
  already uses, or Windows keeps for itself, is warned about but still accepted.
-->
<script lang="ts">
	import { app } from '#lib/stores/app.svelte.ts';

	let { value, label, onchange }: { value: string; label: string; onchange: (v: string) => void } = $props();
	let listening = $state(false);
	let warning = $state('');

	/** Every binding Rewind has: the global ones, the extra slots and each game profile's own. */
	function bindings(): string[] {
		const s = app.settings;
		if (!s) return [];
		return [...Object.values(s.hotkeys), ...s.buffer.extra_slots.filter((x) => x.enabled).map((x) => x.hotkey)].filter(Boolean);
	}

	const norm = (c: string) => c.toLowerCase().replace(/s+/g, '');

	/** Why this combo may not work, or '' when it looks fine. */
	function concerns(combo: string): string {
		const parts = combo.split('+');
		const key = parts[parts.length - 1].toLowerCase();
		const mods = parts.slice(0, -1).map((m) => m.toLowerCase());
		const others = bindings();
		const own = others.findIndex((b) => norm(b) === norm(value));
		if (own >= 0) others.splice(own, 1);
		if (others.some((b) => norm(b) === norm(combo))) return 'Another Rewind hotkey already uses this.';
		if (mods.includes('win')) return 'Windows keeps most Win shortcuts for itself, so this may not fire.';
		if (mods.includes('alt') && ['f4', 'tab', 'space'].includes(key)) return 'Windows uses this to close, switch or open window menus.';
		if (mods.includes('ctrl') && mods.includes('alt') && key === 'delete') return 'Windows reserves this.';
		if (mods.includes('ctrl') && mods.includes('shift') && key === 'escape') return 'Windows reserves this for Task Manager.';
		if (!mods.length && key.length === 1) return 'With no modifier this key stops working for typing everywhere while Rewind runs.';
		if (mods.length === 1 && mods[0] === 'ctrl' && ['c', 'v', 'x', 'z', 'a', 's'].includes(key)) return 'Other apps use this for editing; it will be taken from them.';
		return '';
	}

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
			warning = '';
			onchange('');
			listening = false;
			return;
		}
		const k = keyName(e);
		if (!k) return;
		const parts = [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Win', k].filter(Boolean);
		const combo = parts.join('+');
		warning = concerns(combo);
		onchange(combo);
		listening = false;
	}
</script>

{#if warning}<span class="warn" role="status">{warning}</span>{/if}
{#if value}
	<button class="none" aria-label="Turn off the {label} hotkey" title="Set to None" onclick={() => ((warning = ''), onchange(''))}>None</button>
{/if}
<button
	class="key"
	class:listening
	aria-label="{label}: {value || 'none'}. Click, then press any key combination."
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
		<span class="unset">None</span>
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
	.warn {
		max-width: 30ch;
		color: var(--mark);
		font-size: 12px;
		line-height: 1.3;
		text-align: right;
	}
	.none {
		height: 32px;
		padding: 0 var(--s-2);
		border: 0;
		border-radius: var(--r-1);
		background: transparent;
		color: var(--faint);
		font-size: 12px;
		cursor: pointer;
	}
	.none:hover {
		color: var(--text);
	}
</style>
