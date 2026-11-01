<!--
  Keystroke overlay keycaps, clicks and pluses for one laid-out scene
  (overlay.ts). Drawn by the Settings preview and the live on-screen window,
  so both look like what exports draw. The parent is the positioned frame.
-->
<script lang="ts">
	import type { Keystrokes } from '#lib/api.ts';
	import type { PScene } from '#lib/overlay.ts';

	let { scene, cfg, h }: { scene: PScene; cfg: Keystrokes; h: number } = $props();

	const base = $derived(cfg.style.base_color || '#e7e7e5');
	function lum(hex: string) {
		const v = (i: number) => parseInt(hex.slice(i, i + 2), 16) / 255;
		return 0.2126 * v(1) + 0.7152 * v(3) + 0.0722 * v(5);
	}
	const autoText = $derived(lum(base) > 0.5 ? '#17181a' : '#ffffff');
	const font = $derived(
		{ theme: "'Segoe UI', var(--font-sans)", sans: 'Arial, sans-serif', mono: 'Consolas, ui-monospace, monospace', condensed: 'Bahnschrift, Arial Narrow, sans-serif' }[cfg.style.font]
	);
	const k = $derived(h / 1080);
</script>

{#each scene.clicks as c, i (i)}
	{@const u = scene.unit}
	{#if cfg.position.click_style === 'ring'}
		{@const r = u * 0.18 + Math.min(c.age, 0.4) * u * 1.5}
		<span class="click ring" style="left:{c.x - r}px; top:{c.y - r}px; width:{r * 2}px; height:{r * 2}px; border-width:{Math.max(1.5, u * 0.08)}px; border-color:{base}; opacity:{c.opacity * (1 - Math.min(c.age / 0.6, 1) * 0.5)}"></span>
	{:else if cfg.position.click_style === 'dot'}
		<span class="click" style="left:{c.x - u * 0.22}px; top:{c.y - u * 0.22}px; width:{u * 0.44}px; height:{u * 0.44}px; background:{base}; opacity:{c.opacity * 0.9}"></span>
	{:else}
		<span class="click" style="left:{c.x - u * 0.8}px; top:{c.y - u * 0.8}px; width:{u * 1.6}px; height:{u * 1.6}px; background:#fff; opacity:{0.4 * c.opacity * Math.max(0, 1 - c.age / 0.35)}"></span>
		<span class="click" style="left:{c.x - u * 0.12}px; top:{c.y - u * 0.12}px; width:{u * 0.24}px; height:{u * 0.24}px; background:{base}; opacity:{c.opacity}"></span>
	{/if}
{/each}
{#each scene.caps as c, i (i)}
	{@const outlined = cfg.style.keycap === 'outlined' || (cfg.style.hold_indicator && !c.held && cfg.style.keycap !== 'minimal')}
	{@const fill = c.flash > 0 ? `color-mix(in srgb, ${base}, #fff ${Math.round(c.flash * 70)}%)` : base}
	<div
		class="cap {outlined ? 'outlined' : cfg.style.keycap}"
		class:held={c.held}
		class:shadow={cfg.style.shadow && cfg.style.keycap !== 'minimal'}
		style="left:{c.x}px; top:{c.y}px; width:{c.w}px; height:{c.h}px; opacity:{c.opacity}; transform:scale({c.scale});
			--fill:{fill}; --text:{cfg.style.text_color || (cfg.style.keycap === 'minimal' ? '#fff' : outlined ? fill : autoText)};
			--r:{Math.min(cfg.style.radius * k * (c.h / scene.unit), c.h / 2)}px; --h:{c.h}px; --sh:{(cfg.style.shadow_intensity / 100) * 0.6};
			--bw:{cfg.style.border ? Math.max(1, c.h * 0.05) : 0}px; --bc:{cfg.style.border_color}; font-family:{font};
			font-stretch:{cfg.style.font === 'condensed' ? 'condensed' : 'normal'}"
	>
		<span class="face">{c.label}</span>
	</div>
{/each}
{#each scene.pluses as p, i (i)}
	<span class="plus" style="left:{p.x}px; top:{p.y}px; opacity:{p.opacity}; color:{base}; font-size:{scene.unit * 0.5}px; font-family:{font}">+</span>
{/each}

<style>
	.cap {
		position: absolute;
		display: flex;
		flex-direction: column;
		border-radius: var(--r);
		pointer-events: none;
		box-sizing: border-box;
	}
	.cap::after {
		content: '';
		position: absolute;
		inset: 0;
		border-radius: var(--r);
		box-shadow: inset 0 0 0 var(--bw) var(--bc);
	}
	.cap.shadow {
		box-shadow: 0 calc(var(--h) * 0.07) calc(var(--h) * 0.12) rgb(0 0 0 / var(--sh));
	}
	.face {
		flex: 1;
		display: grid;
		place-items: center;
		min-width: 0;
		border-radius: var(--r);
		color: var(--text);
		font-size: calc(var(--h) * 0.46);
		font-weight: 600;
		line-height: 1;
		white-space: nowrap;
		overflow: hidden;
	}
	.flat .face {
		background: var(--fill);
	}
	.three_d {
		background: color-mix(in srgb, var(--fill) 55%, #000);
	}
	.three_d .face {
		flex: none;
		height: calc(var(--h) * 0.9);
		background: var(--fill);
	}
	.three_d.held .face {
		margin-top: calc(var(--h) * 0.065);
		height: calc(var(--h) * 0.9);
	}
	.minimal .face {
		background: rgb(0 0 0 / 0.45);
	}
	.outlined .face {
		background: rgb(0 0 0 / 0.25);
		box-shadow: inset 0 0 0 max(1.5px, calc(var(--h) * 0.07)) var(--fill);
	}
	.mechanical {
		background: color-mix(in srgb, var(--fill) 62%, #000);
	}
	.mechanical .face {
		margin: calc(var(--h) * 0.05) calc(var(--h) * 0.1) calc(var(--h) * 0.17);
		border-radius: calc(var(--r) * 0.75);
		background: color-mix(in srgb, var(--fill), #fff 12%);
		font-size: calc(var(--h) * 0.36);
	}
	.mechanical.held .face {
		margin-top: calc(var(--h) * 0.1);
		margin-bottom: calc(var(--h) * 0.12);
	}
	.plus {
		position: absolute;
		transform: translate(-50%, -50%);
		font-weight: 600;
		text-shadow: 0 1px 2px rgb(0 0 0 / 0.4);
		pointer-events: none;
	}
	.click {
		position: absolute;
		border-radius: 50%;
		pointer-events: none;
	}
	.click.ring {
		border-style: solid;
		box-sizing: border-box;
	}
</style>
