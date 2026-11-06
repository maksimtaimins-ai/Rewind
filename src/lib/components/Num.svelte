<!-- A number that glides to its new value instead of jumping. Reduced motion: it jumps. -->
<script lang="ts">
	import { Tween } from 'svelte/motion';
	import { cubicOut } from 'svelte/easing';
	import { ms } from '#lib/motion.ts';

	let { value, digits = 0, suffix = '' }: { value: number; digits?: number; suffix?: string } = $props();
	const t = new Tween(0, { duration: 320, easing: cubicOut });
	let first = true;
	$effect(() => {
		const v = value;
		// First value is shown as is; later ones glide.
		t.set(v, first ? { duration: 0 } : { duration: ms(320) });
		first = false;
	});
</script>

<span class="num">{t.current.toFixed(digits)}{suffix}</span>
