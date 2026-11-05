<!-- Recently deleted now lives in Library > Recently deleted; old links land there. -->
<script lang="ts">
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';

	onMount(() => goto('/library?tab=deleted', { replaceState: true }));
</script>
