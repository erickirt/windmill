<script lang="ts">
	import { Alert, Badge, Tab } from '$lib/components/common'
	import TabContent from '$lib/components/common/tabs/TabContent.svelte'
	import TabsV2 from '$lib/components/common/tabs/TabsV2.svelte'

	import type { BranchOne, FlowModule } from '$lib/gen'
	import { Pane, Splitpanes } from 'svelte-splitpanes'
	import FlowCard from '../common/FlowCard.svelte'
	import BranchPredicateEditor from './BranchPredicateEditor.svelte'
	import FlowModuleEarlyStop from './FlowModuleEarlyStop.svelte'
	import FlowModuleDeleteAfterUse from './FlowModuleDeleteAfterUse.svelte'
	import FlowModuleSleep from './FlowModuleSleep.svelte'
	import FlowModuleSuspend from './FlowModuleSuspend.svelte'
	import SplitPanesWrapper from '../../splitPanes/SplitPanesWrapper.svelte'
	import FlowModuleMock from './FlowModuleMock.svelte'
	import { enterpriseLicense } from '$lib/stores'
	import FlowModuleSkip from './FlowModuleSkip.svelte'

	interface Props {
		// import FlowRetries from './FlowRetries.svelte'
		flowModule: FlowModule
		previousModule: FlowModule | undefined
		parentModule: FlowModule | undefined
		noEditor: boolean
		enableAi?: boolean
	}

	let {
		flowModule = $bindable(),
		previousModule,
		parentModule,
		noEditor,
		enableAi = false
	}: Props = $props()

	let value = $state(flowModule.value as BranchOne)
	$effect(() => {
		value = flowModule.value as BranchOne
	})

	let selected = $state('early-stop')
</script>

<div class="h-full" id="flow-editor-branch-one-wrapper">
	<FlowCard {noEditor} title="Run one branch">
		<SplitPanesWrapper>
			<Splitpanes horizontal>
				<Pane size={flowModule ? 60 : 100}>
					{#if !noEditor}
						<Alert
							type="info"
							title="Only first branch whose condition is true will be run"
							tooltip="Branch one"
							documentationLink="https://www.windmill.dev/docs/flows/flow_branches#branch-one"
							class="m-4"
						>
							The result of this step is the result of the branch.
						</Alert>
					{/if}
					<div class="p-4">
						<h3 class="my-4">
							{value.branches.length + 1} branch{value.branches.length + 1 > 1 ? 'es' : ''}
						</h3>
						<div class="py-2">
							<div class="flex flex-row gap-2 text-sm p-2">
								<Badge large={true} color="blue">Default branch</Badge>
								<p class="italic text-tertiary"
									>If none of the predicates' expressions evaluated in-order match, this branch is
									chosen</p
								>
							</div>
							{#each value.branches as branch, i}
								<div class="flex flex-col gap-x-2 items-center">
									<div class="w-full flex gap-2 px-2 pt-4 pb-2">
										<Badge large={true} color="blue">Branch {i + 1}</Badge>
										<input
											class="w-full"
											type="text"
											bind:value={branch.summary}
											placeholder="Summary"
										/>
									</div>
									<div class="w-full border">
										<BranchPredicateEditor
											{branch}
											on:updateSummary={(e) => {
												if (!branch.summary) {
													branch.summary = e.detail
												}
											}}
											parentModule={flowModule}
											{previousModule}
											{enableAi}
										/>
									</div>
								</div>
							{/each}
						</div>
						<p class="text-sm">Add branches and steps directly on the graph.</p>
					</div>
				</Pane>
				{#if flowModule}
					<Pane size={40}>
						<TabsV2 bind:selected>
							<Tab value="early-stop">Early Stop/Break</Tab>
							<Tab value="skip">Skip</Tab>
							<Tab value="suspend">Suspend/Approval/Prompt</Tab>
							<Tab value="sleep">Sleep</Tab>
							<Tab value="mock">Mock</Tab>
							<Tab value="lifetime">Lifetime</Tab>
							{#snippet content()}
								<div class="overflow-hidden bg-surface">
									<TabContent value="early-stop" class="flex flex-col flex-1 h-full">
										<div class="p-4 overflow-y-auto">
											<FlowModuleEarlyStop bind:flowModule />
										</div>
									</TabContent>
									<TabContent value="skip" class="flex flex-col flex-1 h-full">
										<div class="p-4 overflow-y-auto">
											<FlowModuleSkip bind:flowModule {parentModule} {previousModule} />
										</div>
									</TabContent>
									<TabContent value="suspend" class="flex flex-col flex-1 h-full">
										<div class="p-4 overflow-y-auto">
											<FlowModuleSuspend previousModuleId={previousModule?.id} bind:flowModule />
										</div>
									</TabContent>
									<TabContent value="sleep" class="flex flex-col flex-1 h-full">
										<div class="p-4 overflow-y-auto">
											<FlowModuleSleep previousModuleId={previousModule?.id} bind:flowModule />
										</div>
									</TabContent>
									<TabContent value="mock" class="flex flex-col flex-1 h-full">
										<div class="p-4 overflow-y-auto">
											<FlowModuleMock bind:flowModule />
										</div>
									</TabContent>
									<TabContent value="lifetime" class="flex flex-col flex-1 h-full">
										<div class="p-4 overflow-y-auto">
											<FlowModuleDeleteAfterUse bind:flowModule disabled={!$enterpriseLicense} />
										</div>
									</TabContent>
								</div>
							{/snippet}
						</TabsV2>
					</Pane>
				{/if}
			</Splitpanes>
		</SplitPanesWrapper>
	</FlowCard>
</div>
