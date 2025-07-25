import type { Schema } from '$lib/common'
import type { Policy, Preview } from '$lib/gen'
import type { History } from '$lib/history.svelte'

import type { Writable } from 'svelte/store'
import type {
	AppComponent,
	PresetComponentConfig,
	RecomputeOthersSource,
	components
} from './editor/component/components'
import type {
	AppInput,
	ConnectedAppInput,
	ConnectedInput,
	EvalAppInput,
	EvalV2AppInput,
	InputConnection,
	ResultAppInput,
	RowAppInput,
	Runnable,
	StaticAppInput,
	TemplateV2AppInput,
	UploadAppInput,
	UploadS3AppInput,
	UserAppInput
} from './inputType'
import type { World } from './rx'
import type { FilledItem } from './svelte-grid/types'

export type HorizontalAlignment = 'left' | 'center' | 'right'
export type VerticalAlignment = 'top' | 'center' | 'bottom'

export type Aligned = {
	horizontalAlignment: HorizontalAlignment
	verticalAlignment: VerticalAlignment
}

export interface GeneralAppInput {
	tooltip?: string
	placeholder?: string
	customTitle?: string
}

export type ComponentCssProperty = {
	class?: string
	style?: string
	evalClass?: RichConfiguration
}

export type ComponentCustomCSS<T extends keyof typeof components> = Partial<
	(typeof components)[T]['customCss']
>

export type Configuration =
	| StaticAppInput
	| ConnectedAppInput
	| UserAppInput
	| RowAppInput
	| EvalAppInput
	| EvalV2AppInput
	| UploadAppInput
	| UploadS3AppInput
	| ResultAppInput
	| TemplateV2AppInput

export type StaticConfiguration = GeneralAppInput & StaticAppInput
export type OneOfRichConfiguration<T> = {
	type: 'oneOf'
	selected: string
	tooltip?: string
	labels?: Record<string, string>
	configuration: Record<string, Record<string, T>>
}

export type OneOfConfiguration = OneOfRichConfiguration<
	GeneralAppInput & (StaticAppInput | EvalAppInput | EvalV2AppInput)
>

export type RichConfigurationT<T> = (T & { type: AppInput['type'] }) | OneOfRichConfiguration<T>
export type RichConfiguration = RichConfigurationT<Configuration>
export type RichConfigurations = Record<string, RichConfiguration>

export type StaticRichConfigurations = Record<
	string,
	RichConfigurationT<GeneralAppInput & (StaticAppInput | EvalAppInput | EvalV2AppInput)>
>

export interface BaseAppComponent extends Partial<Aligned> {
	id: ComponentID
	componentInput: AppInput | undefined
	configuration: RichConfigurations
	customCss?: Record<string, ComponentCssProperty>
	// Number of subgrids
	numberOfSubgrids?: number
}

export type ComponentSet = {
	title: string
	components: Readonly<AppComponent['type'][]>
	presets?: Readonly<PresetComponentConfig['type'][]> | undefined
}

type SectionID = string

export type AppSection = {
	components: AppComponent[]
	id: SectionID
}

export type GridItem = FilledItem<AppComponent>

export type InlineScript = {
	content: string
	language: Preview['language'] | 'frontend'
	path?: string
	schema?: Schema
	lock?: string
	cache_ttl?: number
	refreshOn?: { id: string; key: string }[]
	suggestedRefreshOn?: { id: string; key: string }[]
	id?: number
}

export type AppCssItemName = 'viewer' | 'grid' | AppComponent['type']

export type HiddenRunnable = {
	name: string
	transformer?: InlineScript & { language: 'frontend' }
	// inlineScript?: InlineScript | undefined
	// type?: 'runnableByName' | 'runnableByPath'
	fields: Record<string, StaticAppInput | ConnectedAppInput | RowAppInput | UserAppInput>
	autoRefresh?: boolean
	//deprecated and to be removed after migration
	doNotRecomputeOnInputChanged?: boolean
	recomputeOnInputChanged?: boolean
	noBackendValue?: any
	hidden?: boolean
} & Runnable &
	RecomputeOthersSource

export type AppTheme =
	| {
			type: 'path'
			path: string
	  }
	| {
			type: 'inlined'
			css: string
	  }

import type { DiffDrawerI } from '$lib/components/diff_drawer'

export interface AppEditorProps {
	app: App
	path: string
	policy: Policy
	summary: string
	fromHub?: boolean
	diffDrawer?: DiffDrawerI | undefined
	savedApp?:
		| {
				value: App
				draft?: any
				path: string
				summary: string
				policy: any
				draft_only?: boolean
				custom_path?: string
		  }
		| undefined
	version?: number | undefined
	newApp?: boolean
	newPath?: string | undefined
	replaceStateFn?: (path: string) => void
	gotoFn?: (path: string, opt?: Record<string, any> | undefined) => void
	unsavedConfirmationModal?: import('svelte').Snippet<[any]>
	onSavedNewAppPath?: (path: string) => void
}

export type App = {
	grid: GridItem[]
	darkMode?: boolean
	fullscreen: boolean
	norefreshbar?: boolean
	unusedInlineScripts: Array<{
		name: string
		inlineScript: InlineScript
	}>

	//TODO: should be called hidden runnables but migration tbd
	hiddenInlineScripts: Array<HiddenRunnable>
	css?: Partial<Record<AppCssItemName, Record<string, ComponentCssProperty>>>
	subgrids?: Record<string, GridItem[]>
	theme: AppTheme | undefined
	lazyInitRequire?: string[] | undefined
	eagerRendering?: boolean | undefined
	hideLegacyTopBar?: boolean | undefined
	mobileViewOnSmallerScreens?: boolean | undefined
	version?: number
}

export type ConnectingInput = {
	opened: boolean
	input?: ConnectedInput
	sourceName?: string
	hoveredComponent: string | undefined
	onConnect?: ((connection: InputConnection) => void) | undefined
}

export interface CancelablePromise<T> extends Promise<T> {
	cancel: () => void
}

export type ListContext = Writable<{
	index: number
	value: any
	disabled: boolean
}>

export type ListInputs = {
	set: (id: string, value: any) => void
	remove: (id: string) => void
}

export type GroupContext = { id: string; context: Writable<Record<string, any>> }

export type JobById = {
	job: string
	component: string
	result?: any
	error?: any
	transformer?: { result?: any; error?: string }
	created_at?: number
	started_at?: number
	duration_ms?: number
}

export type AppViewerContext = {
	worldStore: Writable<World>
	app: Writable<App>
	summary: Writable<string>
	initialized: Writable<{
		initializedComponents: string[]
		initialized: boolean
		runnableInitialized: Record<string, any>
	}>
	selectedComponent: Writable<string[] | undefined>
	mode: Writable<EditorMode>
	connectingInput: Writable<ConnectingInput>
	breakpoint: Writable<EditorBreakpoint>
	bgRuns: Writable<string[]>
	runnableComponents: Writable<
		Record<
			string,
			{
				autoRefresh: boolean
				refreshOnStart?: boolean
				cb: ((inlineScript?: InlineScript, setRunnableJob?: boolean) => CancelablePromise<void>)[]
			}
		>
	>
	staticExporter: Writable<Record<string, () => any>>
	appPath: Writable<string>
	workspace: string
	onchange: (() => void) | undefined
	isEditor: boolean
	jobs: Writable<string[]>
	// jobByComponent: Writable<Record<string, string>>,
	jobsById: Writable<Record<string, JobById>>
	noBackend: boolean
	errorByComponent: Writable<Record<string, { id?: string; error: string }>>
	openDebugRun: Writable<((jobID: string) => void) | undefined>
	focusedGrid: Writable<FocusedGrid | undefined>
	stateId: Writable<number>
	parentWidth: Writable<number>
	state: Writable<Record<string, any>>
	componentControl: Writable<
		Record<
			string,
			{
				left?: () => boolean
				right?: (skipTableActions?: boolean | undefined) => string | boolean
				setTab?: (index: number) => void
				agGrid?: { api: any; columnApi: any }
				setCode?: (value: string) => void
				onDelete?: () => void
				setValue?: (value: any) => void
				setSelectedIndex?: (index: number) => void
				openModal?: () => void
				closeModal?: () => void
				open?: () => void
				close?: () => void
				validate?: (key: string) => void
				invalidate?: (key: string, error: string) => void
				validateAll?: () => void
				clearFiles?: () => void
				showToast?: (message: string, error?: boolean) => void
				recompute?: () => void
				askNewResource?: () => void
				setGroupValue?: (key: string, value: any) => void
			}
		>
	>
	hoverStore: Writable<string | undefined>
	allIdsInPath: Writable<string[]>
	darkMode: Writable<boolean>
	cssEditorOpen: Writable<boolean>
	previewTheme: Writable<string | undefined>
	debuggingComponents: Writable<Record<string, number>>
	replaceStateFn?: ((url: string) => void) | undefined
	gotoFn?: ((url: string, opt?: Record<string, any> | undefined) => void) | undefined
	policy: Policy

	recomputeAllContext: Writable<{
		onRefresh?: (excludeId?: string) => void
		componentNumber?: number | undefined
		interval?: number | undefined
		refreshing?: string[] | undefined
		setInter?: (interval: number) => void | undefined
		progress?: number | undefined
		loading?: boolean | undefined
	}>
	panzoomActive: Writable<boolean>
}

export type AppEditorContext = {
	yTop: Writable<number>
	runnableJobEditorPanel: Writable<{
		focused: boolean
		jobs: Record<string, string>
		frontendJobs: Record<string, any>
		width: number
	}>
	evalPreview: Writable<Record<string, any>>
	componentActive: Writable<boolean>
	dndItem: Writable<Record<string, (x: number, y: number, topY: number) => void>>
	refreshComponents: Writable<(() => void) | undefined>
	history: History<App> | undefined
	pickVariableCallback: Writable<((path: string) => void) | undefined>
	selectedComponentInEditor: Writable<string | undefined>
	movingcomponents: Writable<string[] | undefined>
	jobsDrawerOpen: Writable<boolean>
	scale: Writable<number>
	stylePanel: () => any
}

export type FocusedGrid = { parentComponentId: string; subGridIndex: number }
export type EditorMode = 'dnd' | 'preview'
export type EditorBreakpoint = 'sm' | 'lg'

export const IS_APP_PUBLIC_CONTEXT_KEY = 'isAppPublicContext' as const

type ComponentID = string

export type ContextPanelContext = {
	search: Writable<string>
	manuallyOpened: Writable<Record<string, boolean>>
	hasResult: Writable<Record<string, boolean>>
}
