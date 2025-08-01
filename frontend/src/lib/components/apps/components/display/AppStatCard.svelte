<script lang="ts">
	import { createBubbler, preventDefault } from 'svelte/legacy'

	const bubble = createBubbler()
	import { getContext, untrack } from 'svelte'
	import { initConfig, initOutput } from '../../editor/appUtils'
	import type { AppViewerContext, ComponentCustomCSS, RichConfigurations } from '../../types'
	import { getImageDataURL, initCss } from '../../utils'
	import { components } from '../../editor/component'
	import ResolveConfig from '../helpers/ResolveConfig.svelte'
	import ResolveStyle from '../helpers/ResolveStyle.svelte'
	import { ArrowDown } from 'lucide-svelte'
	import { twMerge } from 'tailwind-merge'
	import { loadIcon } from '../icon'
	import Loader from '../helpers/Loader.svelte'
	import InitializeComponent from '../helpers/InitializeComponent.svelte'

	interface Props {
		id: string
		configuration: RichConfigurations
		customCss?: ComponentCustomCSS<'statcomponent'> | undefined
		render: boolean
	}

	let { id, configuration, customCss = undefined, render }: Props = $props()

	const { app, worldStore } = getContext<AppViewerContext>('AppViewerContext')

	let resolvedConfig = $state(
		initConfig(components['statcomponent'].initialData.configuration, configuration)
	)

	initOutput($worldStore, id, {})

	let css = $state(initCss($app.css?.statcomponent, customCss))

	let iconComponent: any = $state()

	async function handleIcon() {
		if (resolvedConfig?.media?.configuration?.icon?.icon) {
			iconComponent = await loadIcon(
				resolvedConfig?.media?.configuration?.icon?.icon,
				iconComponent,
				34,
				undefined,
				undefined
			)
		}
	}

	let isIcon = $derived(resolvedConfig.media?.selected == 'icon')
	$effect(() => {
		isIcon &&
			resolvedConfig?.media?.configuration?.icon?.icon &&
			iconComponent &&
			untrack(() => handleIcon())
	})
</script>

{#each Object.keys(components['statcomponent'].initialData.configuration) as key (key)}
	<ResolveConfig
		{id}
		{key}
		bind:resolvedConfig={resolvedConfig[key]}
		configuration={configuration[key]}
	/>
{/each}

{#each Object.keys(css ?? {}) as key (key)}
	<ResolveStyle
		{id}
		{customCss}
		{key}
		bind:css={css[key]}
		componentStyle={$app.css?.imagecomponent}
	/>
{/each}

<InitializeComponent {id} />

{#if render}
	<div
		class={twMerge(
			'flex flex-row gap-4 items-center p-4 rounded-md shadow-md h-full',
			css?.container?.class,
			'wm-statistic-card-container'
		)}
		style={css?.container?.style}
	>
		<div
			class={twMerge(
				'flex items-center justify-center w-12 h-12 border rounded-md p-2 text-black',
				css?.media?.class,
				'wm-statistic-card-media'
			)}
			style={css?.media?.style}
		>
			{#if isIcon}
				{#if resolvedConfig?.media}
					{#key resolvedConfig.media}
						<div class="min-w-4 text-primary" bind:this={iconComponent}></div>
					{/key}
				{/if}
			{:else}
				<Loader loading={resolvedConfig?.media?.configuration?.image?.source == undefined}>
					<img
						onpointerdown={preventDefault(bubble('pointerdown'))}
						src={getImageDataURL(
							resolvedConfig?.media?.configuration?.image?.sourceKind,
							resolvedConfig?.media?.configuration?.image?.source
						)}
						alt={resolvedConfig?.title}
					/>
				</Loader>
			{/if}
		</div>

		<div class="w-full">
			<div
				class={twMerge(
					'font-normal text-primary leading-none',
					css?.title?.class,
					'wm-statistic-card-title'
				)}
				style={css?.title?.style}
			>
				{resolvedConfig?.title}
			</div>
			<div class="mt-1 flex items-baseline justify-between">
				<div
					class={twMerge(
						'flex items-baseline text-2xl leading-none font-semibold text-blue-600 dark:text-blue-200',
						css?.value?.class,
						'wm-statistic-card-value'
					)}
					style={css?.value?.style}
				>
					{resolvedConfig?.value}
				</div>

				{#if resolvedConfig?.progress !== undefined && resolvedConfig?.progress !== null && resolvedConfig?.progress !== 0}
					<div
						class={twMerge(
							'flex items-center flex-row gap-2 rounded-full px-2.5 py-0.5 text-sm font-medium',
							resolvedConfig?.progress > 0
								? 'bg-green-100 text-green-800'
								: 'bg-red-100 text-red-800'
						)}
					>
						<ArrowDown
							size={16}
							class={resolvedConfig?.progress > 0 ? 'transform rotate-180' : 'transform rotate-0'}
						/>
						{resolvedConfig?.progress}%
					</div>
				{/if}
			</div>
		</div>
	</div>
{/if}
