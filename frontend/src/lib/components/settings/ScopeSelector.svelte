<script lang="ts">
	import { type ScopeDomain, type ScopeDefinition, TokenService } from '$lib/gen'
	import { sendUserToast } from '$lib/toast'
	import { ChevronRight, Loader2, X } from 'lucide-svelte'
	import Button from '../common/button/Button.svelte'
	import Popover from '../meltComponents/Popover.svelte'
	import Tooltip from '../Tooltip.svelte'
	import { twMerge } from 'tailwind-merge'

	interface Props {
		selectedScopes?: string[]
		disabled?: boolean
		class?: string
	}

	interface ScopeState {
		isSelected: boolean
		resourcePaths: string[]
		currentInputValue: string
		pathError?: string
	}

	interface DomainState {
		isExpanded: boolean
		hasFullAccess: boolean
		scopes: Record<string, ScopeState>
	}

	interface ComponentState {
		domains: Record<string, DomainState>
	}

	let { selectedScopes = $bindable([]), disabled = false, class: className = '' }: Props = $props()

	let scopeDomains = $state<ScopeDomain[] | null>(null)
	let loading = $state(false)
	let error = $state<string | null>(null)

	let componentState = $state<ComponentState>({ domains: {} })

	function createScopeState(scope: ScopeDefinition): ScopeState {
		return {
			isSelected: false,
			resourcePaths: [],
			currentInputValue: '',
			pathError: undefined
		}
	}

	function createDomainState(domain: ScopeDomain): DomainState {
		const scopes: Record<string, ScopeState> = {}
		for (const scope of domain.scopes) {
			scopes[scope.value] = createScopeState(scope)
		}
		return {
			isExpanded: false,
			hasFullAccess: false,
			scopes
		}
	}

	function getScopeState(scopeValue: string): ScopeState | undefined {
		for (const domainState of Object.values(componentState.domains)) {
			if (domainState.scopes[scopeValue]) {
				return domainState.scopes[scopeValue]
			}
		}
		return undefined
	}

	function isScopeDisabled(scope: ScopeDefinition, domain: ScopeDomain): boolean {
		const domainState = getDomainState(domain.name)
		if (!domainState) return false

		if (domainState.hasFullAccess && scope.value.endsWith(':read')) {
			return true
		}

		if (scope.value.endsWith(':read')) {
			const writeScope = scope.value.replace(':read', ':write')
			const writeScopeState = domainState.scopes[writeScope]
			if (writeScopeState?.isSelected) {
				return true
			}
		}

		return false
	}

	function getDomainState(domainName: string): DomainState | undefined {
		return componentState.domains[domainName]
	}

	async function fetchScopeDomains(): Promise<void> {
		loading = true
		error = null

		try {
			scopeDomains = await TokenService.listAvailableScopes()
			initializeDomainStates()
		} catch (err) {
			console.error('Error fetching scope domains:', err)
			sendUserToast('Failed to load scope options', true)
			error = 'Failed to load scope options'
		} finally {
			loading = false
		}
	}

	function initializeDomainStates() {
		if (!scopeDomains) return

		const newDomains: Record<string, DomainState> = {}

		for (const domain of scopeDomains) {
			const domainState = createDomainState(domain)

			const writeScopeValue = getWriteScopeForDomain(domain)
			const hasWriteSelected =
				writeScopeValue &&
				selectedScopes.some(
					(scope) => scope === writeScopeValue || scope.startsWith(writeScopeValue + ':')
				)

			const runScopes = domain.scopes.filter((scope) => scope.value.includes(':run:'))

			const hasRunScopesSelected =
				runScopes.length === 0 ||
				runScopes.every((runScope) =>
					selectedScopes.some(
						(scope) => scope === runScope.value || scope.startsWith(runScope.value + ':')
					)
				)

			domainState.hasFullAccess = Boolean(hasWriteSelected && hasRunScopesSelected)

			// Initialize individual scope states
			for (const scope of domain.scopes) {
				const scopeState = domainState.scopes[scope.value]

				const isSelected = selectedScopes.some((selected) => {
					if (scope.requires_resource_path && selected.startsWith(scope.value + ':')) {
						const resourcePath = selected.substring(scope.value.length + 1)
						const paths = resourcePath.split(',').map((p) => p.trim())
						scopeState.resourcePaths = [...scopeState.resourcePaths, ...paths]
						return true
					}
					return selected === scope.value
				})

				scopeState.isSelected = isSelected
			}

			newDomains[domain.name] = domainState
		}

		componentState = { domains: newDomains }
	}

	function getWriteScopeForDomain(domain: ScopeDomain): string | null {
		const writeScope = domain.scopes.find((scope) => scope.value.endsWith(':write'))
		return writeScope?.value || null
	}

	function toggleDomainExpansion(domainName: string) {
		const domainState = getDomainState(domainName)
		if (domainState) {
			domainState.isExpanded = !domainState.isExpanded
		}
	}

	function handleDomainCheckboxChange(domain: ScopeDomain, checked: boolean) {
		const writeScopeValue = getWriteScopeForDomain(domain)
		if (!writeScopeValue) return

		const domainState = getDomainState(domain.name)
		if (!domainState) return

		domainState.hasFullAccess = checked

		if (checked) {
			selectedScopes = selectedScopes.filter(
				(scope) =>
					!domain.scopes.some(
						(domainScope) =>
							scope === domainScope.value || scope.startsWith(domainScope.value + ':')
					)
			)

			const writeScope = domain.scopes.find((s) => s.value === writeScopeValue)
			if (writeScope?.requires_resource_path) {
				selectedScopes = [...selectedScopes, `${writeScopeValue}`]
			} else {
				selectedScopes = [...selectedScopes, writeScopeValue]
			}

			const runScopes = domain.scopes.filter((scope) => scope.value.includes(':run:'))
			for (const runScope of runScopes) {
				if (runScope.requires_resource_path) {
					selectedScopes = [...selectedScopes, `${runScope.value}`]
				} else {
					selectedScopes = [...selectedScopes, runScope.value]
				}
			}

			for (const scope of domain.scopes) {
				const scopeState = domainState.scopes[scope.value]
				if (scopeState) {
					if (scope.value === writeScopeValue || runScopes.some((rs) => rs.value === scope.value)) {
						scopeState.isSelected = true
					}
				}
			}
		} else {
			// Remove all scopes for this domain
			selectedScopes = selectedScopes.filter(
				(scope) =>
					!domain.scopes.some(
						(domainScope) =>
							scope === domainScope.value || scope.startsWith(domainScope.value + ':')
					)
			)
		}
	}

	function handleIndividualScopeChange(scope: ScopeDefinition, checked: boolean) {
		const scopeState = getScopeState(scope.value)
		if (!scopeState) return

		scopeState.isSelected = checked

		if (scope.requires_resource_path) {
			if (!checked) {
				scopeState.resourcePaths = []
				updateSelectedScopesForResourcePaths(scope.value, [])
			} else {
				const currentPaths = scopeState.resourcePaths || []
				updateSelectedScopesForResourcePaths(scope.value, currentPaths, false)
			}
		} else {
			selectedScopes = selectedScopes.filter(
				(s) => !s.startsWith(scope.value + ':') && s !== scope.value
			)
			if (checked) {
				selectedScopes = [...selectedScopes, scope.value]
			}
		}

		updateDomainCheckboxState(scope)
	}

	function updateDomainCheckboxState(changedScope: ScopeDefinition) {
		if (!scopeDomains) return

		const domain = scopeDomains.find((d) => d.scopes.some((s) => s.value === changedScope.value))
		if (!domain) return

		const domainState = getDomainState(domain.name)
		if (!domainState) return

		const writeScope = getWriteScopeForDomain(domain)
		const hasWriteSelected = writeScope && domainState.scopes[writeScope]?.isSelected

		const runScopes = domain.scopes.filter((scope) => scope.value.includes(':run:'))

		const hasRunScopesSelected =
			runScopes.length === 0 ||
			runScopes.every((runScope) => domainState.scopes[runScope.value]?.isSelected)

		const isDomainFullySelected = hasWriteSelected && hasRunScopesSelected
		domainState.hasFullAccess = Boolean(isDomainFullySelected)
	}

	function getSelectedScopesForDomain(domain: ScopeDomain): string[] {
		const domainState = getDomainState(domain.name)
		if (!domainState) return []

		return domain.scopes
			.filter((scope) => domainState.scopes[scope.value]?.isSelected)
			.map((scope) => {
				const scopeState = domainState.scopes[scope.value]
				const resourcePaths = scopeState?.resourcePaths || []
				return resourcePaths.length > 0 ? `${scope.value}:${resourcePaths.join(',')}` : scope.value
			})
	}

	function removeSelectedScope(scopeToRemove: string) {
		selectedScopes = selectedScopes.filter((scope) => scope !== scopeToRemove)

		const baseScopeValue =
			scopeToRemove.includes(':') && scopeToRemove.split(':').length > 2
				? scopeToRemove.split(':').slice(0, 2).join(':')
				: scopeToRemove

		const scopeState = getScopeState(baseScopeValue)
		if (scopeState) {
			if (scopeToRemove.includes(':') && scopeToRemove.split(':').length > 2) {
				const pathPart = scopeToRemove.substring(baseScopeValue.length + 1)
				const pathsToRemove = pathPart.split(',').map((p) => p.trim())
				scopeState.resourcePaths = scopeState.resourcePaths.filter(
					(path) => !pathsToRemove.includes(path)
				)

				if (scopeState.resourcePaths.length === 0) {
					scopeState.isSelected = false
				}
			} else {
				scopeState.isSelected = false
				scopeState.resourcePaths = []
			}

			updateDomainCheckboxState({ value: baseScopeValue } as ScopeDefinition)
		}
	}

	function clearAllScopes() {
		selectedScopes = []
		for (const domainState of Object.values(componentState.domains)) {
			domainState.hasFullAccess = false
			domainState.isExpanded = false
			for (const scopeState of Object.values(domainState.scopes)) {
				scopeState.isSelected = false
				scopeState.resourcePaths = []
				scopeState.currentInputValue = ''
				scopeState.pathError = undefined
			}
		}
	}

	const hasAdministratorScope = $derived(selectedScopes.includes('*'))

	$effect(() => {
		if (scopeDomains && componentState.domains) {
			syncSelectedScopesWithState()
		}
	})

	function validateResourcePath(path: string): string | null {
		if (!path.trim()) return 'Path cannot be empty'

		const trimmedPath = path.trim()

		if (trimmedPath === '*') return null

		if (trimmedPath === 'u/*' || trimmedPath === 'f/*') return null

		if (!trimmedPath.startsWith('u/') && !trimmedPath.startsWith('f/')) {
			return 'Path must start with u/ or f/'
		}

		const parts = trimmedPath.split('/')
		if (parts.length !== 3) {
			return 'Path must have exactly 3 parts: u/{user}/{resource} or f/{folder}/{resource}'
		}

		if (parts[1] === '') {
			return 'Username/folder name cannot be empty'
		}

		if (parts[2] === '') {
			return 'Resource name cannot be empty'
		}

		if (parts[2] === '*') return null

		if (parts[2].includes('*')) {
			return 'Wildcards can only be used as the full resource name (*)'
		}

		return null
	}

	function addResourcePath(scopeValue: string, path: string) {
		const scopeState = getScopeState(scopeValue)
		if (!scopeState) return false

		const error = validateResourcePath(path)
		if (error) {
			scopeState.pathError = error
			return false
		}

		scopeState.pathError = undefined

		if (scopeState.resourcePaths.includes(path.trim())) {
			scopeState.pathError = 'Path already exists'
			return false
		}

		const newPaths = [...scopeState.resourcePaths, path.trim()]
		scopeState.resourcePaths = newPaths
		scopeState.currentInputValue = ''

		updateSelectedScopesForResourcePaths(scopeValue, newPaths)
		return true
	}

	function removeResourcePath(scopeValue: string, pathToRemove: string) {
		const scopeState = getScopeState(scopeValue)
		if (!scopeState) return

		const newPaths = scopeState.resourcePaths.filter((p) => p !== pathToRemove)
		scopeState.resourcePaths = newPaths
		scopeState.pathError = undefined

		updateSelectedScopesForResourcePaths(scopeValue, newPaths, false)
	}

	function updateSelectedScopesForResourcePaths(
		scopeValue: string,
		paths: string[],
		removeScope = true
	) {
		selectedScopes = selectedScopes.filter(
			(s) => !s.startsWith(scopeValue + ':') && s !== scopeValue
		)

		const scopeState = getScopeState(scopeValue)
		if (!scopeState) return

		if (paths.length > 0 || !removeScope) {
			selectedScopes = [
				...selectedScopes,
				paths.length > 0 ? `${scopeValue}:${paths.join(',')}` : scopeValue
			]
			scopeState.isSelected = true
		} else {
			scopeState.isSelected = false
		}

		updateDomainCheckboxState({ value: scopeValue } as ScopeDefinition)
	}

	function syncSelectedScopesWithState() {
		if (!scopeDomains) return

		for (const domain of scopeDomains) {
			const domainState = getDomainState(domain.name)
			if (!domainState) continue

			const writeScopeValue = getWriteScopeForDomain(domain)
			const hasWriteSelected =
				writeScopeValue &&
				selectedScopes.some(
					(scope) => scope === writeScopeValue || scope.startsWith(writeScopeValue + ':')
				)

			const runScopes = domain.scopes.filter((scope) => scope.value.includes(':run:'))

			const hasRunScopesSelected =
				runScopes.length === 0 ||
				runScopes.every((runScope) =>
					selectedScopes.some(
						(scope) => scope === runScope.value || scope.startsWith(runScope.value + ':')
					)
				)

			domainState.hasFullAccess = Boolean(hasWriteSelected && hasRunScopesSelected)

			for (const scope of domain.scopes) {
				const scopeState = domainState.scopes[scope.value]
				if (!scopeState) continue

				scopeState.resourcePaths = []

				const isSelected = selectedScopes.some((selected) => {
					if (scope.requires_resource_path && selected.startsWith(scope.value + ':')) {
						const resourcePath = selected.substring(scope.value.length + 1)
						const paths = resourcePath.split(',').map((p) => p.trim())
						scopeState.resourcePaths = [...paths]
						return true
					}
					return selected === scope.value
				})

				scopeState.isSelected = isSelected
			}
		}
	}

	fetchScopeDomains()
</script>

<div class="w-full {className} p-2">
	{#if loading}
		<div class="flex items-center justify-center py-12">
			<Loader2 size={32} class="animate-spin text-primary" />
		</div>
	{:else if error}
		<div class="p-4 bg-red-50 border border-red-200 rounded-lg">
			<p class="text-sm text-red-800 mb-3">{error}</p>
			<Button onclick={fetchScopeDomains} variant="contained" color="red" size="sm">
				Try again
			</Button>
		</div>
	{:else if scopeDomains}
		<div class="mb-6 p-4 bg-surface-secondary border rounded-lg">
			<div class="flex items-center justify-between mb-3">
				<h4 class="text-sm font-semibold text-primary">
					Selected Scopes ({selectedScopes.length})
				</h4>
				<Button onclick={clearAllScopes} {disabled} size="xs" color="light">Clear All</Button>
			</div>

			{#if selectedScopes.length === 0}
				<p class="text-sm text-tertiary">No scopes selected. Token will have full access.</p>
			{:else if hasAdministratorScope}
				<p class="text-sm text-tertiary">Administrator scope grants full access to all resources.</p
				>
			{:else}
				<div class="flex flex-wrap gap-2">
					{#each selectedScopes.slice(0, 10) as scope}
						<span
							class="inline-flex items-center gap-1 px-1.5 py-0.5 text-xs font-medium bg-blue-100 text-blue-800 rounded font-mono"
						>
							{scope}
							<button
								type="button"
								onclick={() => removeSelectedScope(scope)}
								class="text-blue-600 hover:text-blue-800 flex-shrink-0"
								title="Remove scope"
								{disabled}
							>
								<X size={10} />
							</button>
						</span>
					{/each}
					{#if selectedScopes.length > 10}
						<span
							class="inline-flex items-center px-2.5 py-0.5 text-xs font-medium bg-surface text-secondary rounded"
						>
							+{selectedScopes.length - 10} more
						</span>
					{/if}
				</div>
			{/if}
		</div>

		<div class="space-y-3 max-h-96 overflow-y-auto border rounded-lg p-4">
			{#each scopeDomains as domain}
				{@const domainState = getDomainState(domain.name)}
				{@const isExpanded = domainState?.isExpanded || false}
				{@const isDomainSelected = domainState?.hasFullAccess || false}
				{@const selectedScopes = getSelectedScopesForDomain(domain)}

				<div class="border rounded-lg bg-surface overflow-hidden">
					<!-- svelte-ignore a11y_click_events_have_key_events -->
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<div
						class="p-4 bg-surface-secondary cursor-pointer hover:bg-surface-tertiary transition-colors"
						onclick={() => toggleDomainExpansion(domain.name)}
					>
						<div class="flex items-center gap-3">
							<div class="flex-shrink-0">
								<ChevronRight
									size={16}
									class="text-secondary transition-transform duration-200 {isExpanded
										? 'rotate-90'
										: ''}"
								/>
							</div>

							<div class="flex-shrink-0">
								<input
									type="checkbox"
									id={`domain-${domain.name}`}
									checked={isDomainSelected}
									{disabled}
									onchange={(e) => handleDomainCheckboxChange(domain, e.currentTarget.checked)}
									onclick={(e) => e.stopPropagation()}
									class="!w-4 !h-4 cursor-pointer"
								/>
							</div>

							<div class="flex-1 min-w-0">
								<div class="flex items-center gap-2 flex-wrap">
									<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
									<label
										for={`domain-${domain.name}`}
										class="text-sm font-semibold text-primary cursor-pointer"
										onclick={(e) => e.stopPropagation()}
									>
										{domain.name}
									</label>
									{#each selectedScopes as scope}
										<span
											class="inline-flex items-center gap-1 px-1.5 py-0.5 text-xs font-medium bg-blue-100 text-blue-800 rounded font-mono"
										>
											{scope}
											<button
												type="button"
												onclick={(e) => {
													e.stopPropagation()
													removeSelectedScope(scope)
												}}
												class="text-blue-600 hover:text-blue-800 flex-shrink-0"
												title="Remove scope"
												{disabled}
											>
												<X size={10} />
											</button>
										</span>
									{/each}
								</div>
								{#if domain.description}
									<p class="text-xs text-tertiary mt-0.5">{domain.description}</p>
								{/if}
							</div>
						</div>
					</div>

					{#if isExpanded}
						<div class="p-2">
							<div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
								{#each domain.scopes as scope}
									{@const scopeState = domainState?.scopes[scope.value]}
									{@const isSelected = scopeState?.isSelected || false}
									{@const resourcePathArray = scopeState?.resourcePaths || []}
									{@const currentInput = scopeState?.currentInputValue || ''}
									{@const pathError = scopeState?.pathError}
									{@const isDisabled = disabled || isScopeDisabled(scope, domain)}

									<div
										class="p-3 border rounded-lg w-full {isDisabled
											? 'bg-surface opacity-60'
											: 'bg-surface-secondary'}"
									>
										<div class="flex justify-between items-center mb-2">
											<label
												class={twMerge(
													'flex items-center gap-2 flex-1 min-w-0',
													isDisabled ? 'cursor-not-allowed' : 'cursor-pointer'
												)}
											>
												<input
													type="checkbox"
													checked={isSelected}
													disabled={isDisabled}
													onchange={(e) =>
														handleIndividualScopeChange(scope, e.currentTarget.checked)}
													class="!w-4 !h-4 flex-shrink-0"
												/>

												<span
													class={twMerge(
														'font-medium text-xs truncate cursor-pointer',
														isDisabled ? 'text-tertiary' : ''
													)}
												>
													{scope.label}
												</span>
											</label>
											<div class="flex-shrink-0">
												{#if scope.requires_resource_path}
													<Popover
														disabled={isDisabled}
														closeOnOtherPopoverOpen
														contentClasses="p-3"
													>
														{#snippet trigger()}
															<Button size="xs" disabled={isDisabled} color="dark">
																Restrict paths
																<Tooltip light>
																	Restrict this scope to specific resource paths. If no paths are
																	specified, the scope gives full access.
																</Tooltip>
															</Button>
														{/snippet}
														{#snippet content()}
															<div class="w-80">
																<div class="flex gap-2">
																	<input
																		type="text"
																		value={currentInput}
																		{disabled}
																		oninput={(e) => {
																			if (scopeState) {
																				scopeState.currentInputValue = e.currentTarget.value
																				scopeState.pathError = undefined
																			}
																		}}
																		placeholder="e.g. f/folder/*, u/user/path"
																		onkeydown={(e) => {
																			if (e.key === 'Enter' && currentInput.trim()) {
																				e.preventDefault()
																				addResourcePath(scope.value, currentInput)
																			}
																		}}
																		class="flex-1 text-sm px-3 py-2 border rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-blue-500 bg-surface"
																	/>
																	<Button
																		onclick={() => {
																			addResourcePath(scope.value, currentInput)
																		}}
																		size="xs"
																		disabled={!currentInput.trim()}
																	>
																		Add
																	</Button>
																</div>
																{#if pathError}
																	<p class="text-xs text-red-600 mt-1">{pathError}</p>
																{/if}
															</div>
														{/snippet}
													</Popover>
												{/if}
											</div>
										</div>

										{#if scope.requires_resource_path && resourcePathArray.length > 0}
											<div class="flex flex-wrap gap-1 mt-2">
												{#each resourcePathArray as path}
													<span
														class="inline-flex items-center gap-1 px-1.5 py-0.5 text-xs font-medium bg-blue-100 text-blue-800 rounded font-mono"
													>
														{path}
														<button
															type="button"
															onclick={() => removeResourcePath(scope.value, path)}
															class="text-blue-600 hover:text-blue-800 flex-shrink-0"
															title="Remove path"
														>
															<X size={10} />
														</button>
													</span>
												{/each}
											</div>
										{/if}
									</div>
								{/each}
							</div>
						</div>
					{/if}
				</div>
			{/each}
		</div>
	{/if}
</div>
