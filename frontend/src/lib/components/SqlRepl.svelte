<script module lang="ts">
	// code may be composed of many sql statements separated by ';'
	// this function splits them while taking into account that ';' may not
	// be the end of a statement (string or escaped)
	function splitSqlStatements(code: string) {
		const statements: string[] = []
		let currentStatement = ''
		let inSingleQuote = false
		let inDoubleQuote = false
		let inBacktick = false

		for (let i = 0; i < code.length; i++) {
			const char = code[i]
			const prevChar = i > 0 ? code[i - 1] : null

			if (char === "'" && !inDoubleQuote && !inBacktick && prevChar !== '\\') {
				inSingleQuote = !inSingleQuote
			} else if (char === '"' && !inSingleQuote && !inBacktick && prevChar !== '\\') {
				inDoubleQuote = !inDoubleQuote
			} else if (char === '`' && !inSingleQuote && !inDoubleQuote && prevChar !== '\\') {
				inBacktick = !inBacktick
			}

			if (char === ';' && !inSingleQuote && !inDoubleQuote && !inBacktick) {
				statements.push(currentStatement.trim())
				currentStatement = ''
			} else {
				currentStatement += char
			}
		}

		if (currentStatement.trim()) {
			statements.push(currentStatement.trim())
		}

		return statements
	}

	function pruneComments(code: string) {
		return code
			.replace(/--.*?(\r?\n|$)/g, '')
			.replace(/\/\*[\s\S]*?\*\//g, '')
			.trim()
	}
</script>

<script lang="ts">
	import { CornerDownLeft, Loader2 } from 'lucide-svelte'
	import Button from './common/button/Button.svelte'
	import type Editor from './Editor.svelte'
	import { runScriptAndPollResult } from './jobs/utils'
	import { workspaceStore } from '$lib/stores'
	import { sendUserToast } from '$lib/toast'
	import { untrack } from 'svelte'
	import { getLanguageByResourceType } from './apps/components/display/dbtable/utils'
	import StepHistory, { type StepHistoryData } from './flows/propPicker/StepHistory.svelte'
	import { Pane, Splitpanes } from 'svelte-splitpanes'

	type Props = {
		resourceType: string
		resourcePath: string
		onData: (data: Record<string, any>[]) => void
		placeholderTableName?: string
	}
	let { resourcePath, resourceType, onData, placeholderTableName }: Props = $props()

	const DEFAULT_SQL = 'SELECT * FROM _'
	let code = $state(DEFAULT_SQL)
	$effect(() => {
		const _code = untrack(() => code)
		if (placeholderTableName && _code === DEFAULT_SQL) {
			code = _code.replace('_', placeholderTableName ?? 'table')
		}
	})
	let isRunning = $state(false)

	let runHistory: (StepHistoryData & { code: string; result: Record<string, any>[] })[] = $state([])

	async function run({ doPostgresRowToJsonFix }: { doPostgresRowToJsonFix?: boolean } = {}) {
		if (isRunning || !$workspaceStore) return
		isRunning = true
		try {
			const statements = splitSqlStatements(pruneComments(code))
			if (statements.length === 0) {
				sendUserToast('Nothing to run', true)
				return
			}

			// Transform all to JSON in case of select. This fixes the issue of
			// custom postgres enum type failing to convert to a rust type in the backend.
			// We don't always put the fix by default for row ordering concerns
			let transformedCode = code
			if (doPostgresRowToJsonFix) {
				transformedCode = statements
					.map((statement) => {
						if (statement.trim().toUpperCase().startsWith('SELECT')) {
							return `SELECT row_to_json(__t__) FROM (${statement}) __t__`
						}
						return statement
					})
					.join(';')
			}

			let { job, result } = (await runScriptAndPollResult(
				{
					workspace: $workspaceStore,
					requestBody: {
						language: getLanguageByResourceType(resourceType),
						content: transformedCode,
						args: {
							database: '$res:' + resourcePath
						}
					}
				},
				{ withJobData: true }
			)) as any
			if (statements.length > 1) {
				result = result[result.length - 1]
			}
			if (!Array.isArray(result)) {
				sendUserToast('Query result is not an array', true)
				return
			}

			if (doPostgresRowToJsonFix) {
				result = result.map((row: any) => row['row_to_json'])
			}

			if (statements[statements.length - 1].toUpperCase().trim().startsWith('SELECT')) {
				runHistory.push({
					created_at: new Date().toISOString(),
					created_by: '',
					id: job.id,
					success: true,
					code,
					result
				})
				onData(result)
			}
			if (doPostgresRowToJsonFix)
				sendUserToast('Query failed but recovered with the row_to_json fix')
			else sendUserToast('Query executed')
		} catch (e) {
			console.error(e)
			if (resourceType === 'postgresql' && !doPostgresRowToJsonFix) {
				console.error('Error running query, trying with row_to_json fix')
				isRunning = false
				return await run({ doPostgresRowToJsonFix: true })
			}
			sendUserToast('Error running query: ' + (e.message ?? e.error.message), true)
		} finally {
			isRunning = false
		}
	}
	let editor = $state<Editor | null>(null)
</script>

<Splitpanes>
	<Pane class="relative">
		{#await import('$lib/components/Editor.svelte')}
			<Loader2 class="animate-spin" />
		{:then Module}
			<Module.default
				bind:this={editor}
				bind:code
				scriptLang="mysql"
				class="w-full h-full"
				cmdEnterAction={run}
			/>
		{/await}
		<Button
			wrapperClasses="absolute z-10 bottom-2 right-6"
			color={isRunning ? 'red' : undefined}
			variant="border"
			shortCut={{ Icon: CornerDownLeft }}
			on:click={() => run()}
		>
			{isRunning ? 'Running...' : 'Run'}
		</Button>
	</Pane>
	<Pane size={24} minSize={16}>
		<StepHistory
			staticInputs={runHistory}
			on:select={(e) => {
				const data = e.detail as (typeof runHistory)[number]
				editor?.setCode(data.code)
				onData(data.result)
			}}
		/>
	</Pane>
</Splitpanes>
