import { BaseParamsSchema, DiffSpec } from './shared.js';
import { listStackBranches, listStacks } from './status.js';
import { executeGitButlerCommand, hasGitButlerExecutable } from '../../shared/command.js';
import { CallToolResult, GetPromptResult } from '@modelcontextprotocol/sdk/types.js';
import { z } from 'zod';
import { zodToJsonSchema } from 'zod-to-json-schema';

const CommitParamsSchema = BaseParamsSchema.extend({
	message: z.string({ description: 'The commit message' }),
	all: z.boolean().optional().default(false),
	filePaths: z
		.array(z.string(), {
			description: 'The paths of files to commit. These have to be relative paths.'
		})
		.optional()
		.default([]),
	branch: z.string({ description: 'The branch to commit to' })
});

type CommitParams = z.infer<typeof CommitParamsSchema>;

/**
 * Commit changes.
 */
function commit(params: CommitParams) {
	const args = ['commit', '--message', params.message];

	if (params.all) {
		if (params.filePaths.length > 0) {
			throw new Error('Cannot use --all and file paths together');
		}
	}

	if (params.filePaths.length > 0) {
		const diffSpec: DiffSpec[] = [];
		for (const filePath of params.filePaths) {
			diffSpec.push({
				pathBytes: filePath,
				hunkHeaders: []
			});
		}

		const diffSpecJson = JSON.stringify(diffSpec);
		args.push('--diff-spec', diffSpecJson);
	}

	const stacks = listStacks({ project_directory: params.project_directory });

	if (stacks.length === 0) {
		throw new Error('No stacks found');
	}

	for (const stack of stacks) {
		const heads = stack.heads.map((h) => h.name);
		if (heads.includes(params.branch)) {
			if (heads.length === 1) {
				// If this stack has only one branch, we can commit directly to it
				args.push('-s', params.branch);
				return executeGitButlerCommand(params.project_directory, args, undefined);
			}

			const stackBranches = listStackBranches({
				project_directory: params.project_directory,
				stack_id: stack.id
			});
			if (stackBranches.length === 0) {
				throw new Error(`No branches found in stack ${stack.id}`);
			}

			const branch = stackBranches.find((b) => b.name === params.branch);
			if (!branch) {
				throw new Error(`Branch ${params.branch} not found in stack ${stack.id}`);
			}

			if (branch.archived) {
				throw new Error(`Branch ${params.branch} is archived`);
			}

			args.push('-s', params.branch);
			return executeGitButlerCommand(params.project_directory, args, undefined);
		}
	}

	throw new Error(`Branch ${params.branch} not found in any stack`);
}

const TOOL_LISTINGS = [
	{
		name: 'commit',
		description: 'Commit a set of changes to a specific branch in the GitButler project.',
		inputSchema: zodToJsonSchema(CommitParamsSchema)
	}
] as const;

type ToolName = (typeof TOOL_LISTINGS)[number]['name'];

function isToolName(name: string): name is ToolName {
	return TOOL_LISTINGS.some((tool) => tool.name === name);
}

export function getCommitToolListing() {
	if (!hasGitButlerExecutable()) {
		return [];
	}

	return TOOL_LISTINGS;
}

export async function getCommitToolRequestHandler(
	toolName: string,
	params: Record<string, unknown>
): Promise<CallToolResult | null> {
	if (!isToolName(toolName) || !hasGitButlerExecutable()) {
		return null;
	}

	switch (toolName) {
		case 'commit': {
			try {
				const parsedParams = CommitParamsSchema.parse(params);
				commit(parsedParams);
				return { content: [{ type: 'text', text: 'Commit successful' }] };
			} catch (error: unknown) {
				if (error instanceof Error) {
					return { content: [{ type: 'text', text: `Error: ${error.message}` }], isError: true };
				}

				return { content: [{ type: 'text', text: `Error: ${String(error)}` }], isError: true };
			}
		}
	}
}

const PROMPTS = [
	{
		name: 'commit',
		description: `Commit the file changes into the right stack and branch.
This will create and propose a commit plan for the changes in the project.
If there's any ambiguity, will ask the user for clarification.`,
		arguments: [
			{
				name: 'disambiguation',
				description:
					'Any kind of additional information that can help to disambiguate what and how the commits should be done.',
				required: false
			}
		]
	}
] as const;

function isCommitPromptParams(
	params: Record<string, unknown>
): params is { disambiguation?: string } {
	return typeof params.disambiguation === 'string' || typeof params.disambiguation === 'undefined';
}

function buildCommitPrompt(params: Record<string, unknown>): GetPromptResult {
	const disambiguation = isCommitPromptParams(params) ? params.disambiguation : undefined;
	const suffix = disambiguation ? '\nImportantly: ' + disambiguation : '';

	return {
		messages: [
			{
				role: 'user',
				content: {
					type: 'text',
					text: `I want to commit the changes in my project using GitButler.
Follow these instructions to do so:
1. List and take a look at the file changes in the project.
2. Determine to which branch (or branches) to commit what. For that, you can list the stacks and branches in the project and take a look at their names.
3. Create a commit plan for the changes. By commit plan, I mean a list of commits that will be created based off the changes listed above.
4. Propose the commit plan to me, including the target branch (or branches), commit messages and the files that will be included in each commit.
5. If there's any ambiguity, ask me for clarification.
6. If I accept, commit as planned.${suffix}`
				}
			}
		]
	};
}

type PromptName = (typeof PROMPTS)[number]['name'];

function isPromptName(name: string): name is PromptName {
	return PROMPTS.some((prompt) => prompt.name === name);
}

export function getCommitToolPrompts() {
	if (!hasGitButlerExecutable()) {
		return [];
	}

	return PROMPTS;
}

export async function getCommitToolPromptRequestHandler(
	promptName: string,
	params: Record<string, unknown>
): Promise<GetPromptResult | null> {
	if (!isPromptName(promptName) || !hasGitButlerExecutable()) {
		return null;
	}

	switch (promptName) {
		case 'commit': {
			return buildCommitPrompt(params);
		}
	}
}
