<script lang="ts">
	import Popover from '$lib/components/meltComponents/Popover.svelte'
	import AvailableContextList from './AvailableContextList.svelte'
	import ContextElementBadge from './ContextElementBadge.svelte'
	import ContextTextarea from './ContextTextarea.svelte'
	import autosize from '$lib/autosize'
	import type { ContextElement } from './context'
	import { aiChatManager } from './AIChatManager.svelte'
	import { twMerge } from 'tailwind-merge'
	import type { Snippet } from 'svelte'

	interface Props {
		availableContext: ContextElement[]
		selectedContext: ContextElement[]
		isFirstMessage?: boolean
		disabled?: boolean
		placeholder?: string
		initialInstructions?: string
		editingMessageIndex?: number | null
		onEditEnd?: () => void
		className?: string
		onClickOutside?: () => void
		onSendRequest?: (instructions: string) => void
		showContext?: boolean
		bottomRightSnippet?: Snippet
		onKeyDown?: (e: KeyboardEvent) => void
	}

	let {
		availableContext,
		selectedContext = $bindable([]),
		disabled = false,
		isFirstMessage = false,
		placeholder = 'Ask anything',
		initialInstructions = '',
		editingMessageIndex = null,
		onEditEnd = () => {},
		className = '',
		onClickOutside = () => {},
		onSendRequest = undefined,
		showContext = true,
		bottomRightSnippet,
		onKeyDown = undefined
	}: Props = $props()

	let contextTextareaComponent: ContextTextarea | undefined = $state()
	let instructionsTextareaComponent: HTMLTextAreaElement | undefined = $state()
	let instructions = $state(initialInstructions)

	export function focusInput() {
		if (aiChatManager.mode === 'script') {
			contextTextareaComponent?.focus()
		} else {
			instructionsTextareaComponent?.focus()
		}
	}

	function clickOutside(node: HTMLElement) {
		function handleClick(event: MouseEvent) {
			if (node && !node.contains(event.target as Node)) {
				onClickOutside()
			}
		}

		document.addEventListener('click', handleClick, true)
		return {
			destroy() {
				document.removeEventListener('click', handleClick, true)
			}
		}
	}

	function addContextToSelection(contextElement: ContextElement) {
		if (
			selectedContext &&
			availableContext &&
			!selectedContext.find(
				(c) => c.type === contextElement.type && c.title === contextElement.title
			) &&
			availableContext.find(
				(c) => c.type === contextElement.type && c.title === contextElement.title
			)
		) {
			selectedContext = [...selectedContext, contextElement]
		}
	}

	function sendRequest() {
		if (aiChatManager.loading) {
			return
		}
		if (editingMessageIndex !== null) {
			aiChatManager.restartGeneration(editingMessageIndex, instructions)
			onEditEnd()
		} else {
			aiChatManager.sendRequest({ instructions })
			instructions = ''
		}
	}

	$effect(() => {
		if (editingMessageIndex !== null) {
			focusInput()
		}
	})
</script>

<div use:clickOutside class="relative">
	{#if aiChatManager.mode === 'script'}
		{#if showContext}
			<div class="flex flex-row gap-1 mb-1 overflow-scroll pt-2 no-scrollbar">
				<Popover>
					<svelte:fragment slot="trigger">
						<div
							class="border rounded-md px-1 py-0.5 font-normal text-tertiary text-xs hover:bg-surface-hover bg-surface"
							>@</div
						>
					</svelte:fragment>
					<svelte:fragment slot="content" let:close>
						<AvailableContextList
							{availableContext}
							{selectedContext}
							onSelect={(element) => {
								addContextToSelection(element)
								close()
							}}
						/>
					</svelte:fragment>
				</Popover>
				{#each selectedContext as element}
					<ContextElementBadge
						contextElement={element}
						deletable
						on:delete={() => {
							selectedContext = selectedContext?.filter(
								(c) => c.type !== element.type || c.title !== element.title
							)
						}}
					/>
				{/each}
			</div>
		{/if}
		<ContextTextarea
			bind:this={contextTextareaComponent}
			bind:value={instructions}
			{availableContext}
			{selectedContext}
			{isFirstMessage}
			{placeholder}
			onAddContext={(contextElement) => addContextToSelection(contextElement)}
			onSendRequest={() => {
				if (disabled) {
					return
				}
				onSendRequest ? onSendRequest(instructions) : sendRequest()
			}}
			{disabled}
			{onKeyDown}
		/>
	{:else}
		<div class={twMerge('relative w-full scroll-pb-2 pt-2', className)}>
			<textarea
				bind:this={instructionsTextareaComponent}
				bind:value={instructions}
				use:autosize
				onkeydown={(e) => {
					if (onKeyDown) {
						onKeyDown(e)
					}
					if (e.key === 'Enter' && !e.shiftKey) {
						e.preventDefault()
						sendRequest()
					}
				}}
				rows={3}
				{placeholder}
				class="resize-none"
				{disabled}
			></textarea>
		</div>
	{/if}
	{#if bottomRightSnippet}
		<div class="absolute bottom-2 right-2">
			{@render bottomRightSnippet()}
		</div>
	{/if}
</div>
