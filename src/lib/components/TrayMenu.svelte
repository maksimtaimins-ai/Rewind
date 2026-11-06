<!--
  Settings → Advanced → Tray menu: which entries the tray icon's right-click
  menu has, in what order, and what a left click does.
-->
<script lang="ts">
	import type { Settings, TrayItemId } from '#lib/api.ts';
	import { app, updateSettings } from '#lib/stores/app.svelte.ts';
	import Row from './Row.svelte';
	import Segmented from './Segmented.svelte';
	import Sortable from './Sortable.svelte';
	import Toggle from './Toggle.svelte';

	const tray = $derived(app.settings!.tray);
	const set = (f: (t: Settings['tray']) => void) => updateSettings((x) => f(x.tray));

	const names: Record<TrayItemId, string> = {
		open: 'Open Rewind',
		save: 'Save clip',
		marker: 'Mark this moment',
		screenshot: 'Take screenshot',
		library: 'Open library',
		settings: 'Open settings',
		folder: 'Open clips folder',
		pause: 'Pause / resume buffer',
		quit: 'Quit Rewind'
	};

	const items = $derived(tray.items.map((i) => ({ ...i })));
	const order = (ids: string[]) =>
		set((t) => {
			t.items = ids.map((id) => t.items.find((i) => i.id === id)!);
		});
</script>

<Row id="s-tray-left" label="Left-click the tray icon" hint="Right-click always opens the menu below.">
	<Segmented
		label="Tray left-click action"
		value={tray.left_click}
		options={[
			{ value: 'open_window' as const, label: 'Open window' },
			{ value: 'save_clip' as const, label: 'Save clip' },
			{ value: 'compact_hud' as const, label: 'Compact HUD' }
		]}
		onchange={(v) => set((t) => (t.left_click = v))}
	/>
</Row>
<div class="list" id="s-tray-menu">
	<p class="intro">Drag to reorder. Switched-off entries stay out of the menu. Quit is always there.</p>
	<Sortable {items} label="Tray menu entries" onchange={order}>
		{#snippet row(it)}
			<span class="name" class:off={!it.enabled}>{names[it.id as TrayItemId]}</span>
			<Toggle checked={it.enabled} disabled={it.id === 'quit'} label="Show {names[it.id as TrayItemId]} in the tray menu" onchange={(v) => set((t) => (t.items.find((i) => i.id === it.id)!.enabled = v))} />
		{/snippet}
	</Sortable>
</div>

<style>
	.list {
		padding: 8px 0 4px;
	}
	.intro {
		color: var(--muted);
		font-size: 13px;
		margin: 0 0 8px;
	}
	.name.off {
		color: var(--muted);
	}
</style>
