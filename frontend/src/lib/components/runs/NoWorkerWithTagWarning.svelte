<script lang="ts">
	import { WorkerService } from '$lib/gen'
	import { AlertTriangle } from 'lucide-svelte'
	import Popover from '../Popover.svelte'
	import { onDestroy } from 'svelte'
	export let tag: string

	let noWorkerWithTag = false

	let timeout: NodeJS.Timeout | undefined = undefined

	let visible = true
	async function lookForTag(): Promise<void> {
		try {
			const existsWorkerWithTag = await WorkerService.existsWorkerWithTag({ tag })
			noWorkerWithTag = !existsWorkerWithTag
			if (noWorkerWithTag) {
				timeout = setTimeout(() => {
					if (visible) {
						lookForTag()
					}
				}, 1000)
			}
		} catch (err) {
			console.error(err)
		}
	}

	lookForTag()

	onDestroy(() => {
		visible = false
		if (timeout) {
			clearTimeout(timeout)
		}
	})
</script>

{#if noWorkerWithTag}
	<Popover notClickable placement="top">
		<AlertTriangle size={16} class="text-yellow-500" />
		<svelte:fragment slot="text">
			No worker with tag <b>{tag}</b> is currently running.
		</svelte:fragment>
	</Popover>
{/if}
