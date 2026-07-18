import { posix as path } from "node:path";
import {
  createBashTool,
  createEditTool,
  createFindTool,
  createLsTool,
  createReadTool,
  createWriteTool,
  DEFAULT_MAX_BYTES,
  formatSize,
  truncateHead,
  truncateLine,
  type BashOperations,
  type EditOperations,
  type FindOperations,
  type GrepToolDetails,
  type LsOperations,
  type ReadOperations,
  type WriteOperations
} from "@earendil-works/pi-coding-agent";
import {
  runAgentLoop,
  type AgentContext as PiAgentContext,
  type AgentEvent as PiAgentEvent,
  type AgentMessage as PiAgentMessage,
  type AgentTool as PiAgentTool,
  type StreamFn
} from "@earendil-works/pi-agent-core";
import {
  Type,
  type Api,
  type AssistantMessage,
  type Message as PiMessage,
  type Model,
  type Usage
} from "@earendil-works/pi-ai";
import { VercelVirtualExecutionEnv, type VirtualFileSnapshotEntry } from "./execution-env";

export const vercelPiCodingToolNames = [
  "read",
  "bash",
  "edit",
  "write",
  "grep",
  "find",
  "ls"
] as const;

export type VercelPiCodingToolName = typeof vercelPiCodingToolNames[number];

export type VercelPiCodingToolsMode = "coding" | "readOnly" | "all";

export type CreateVercelPiCodingToolsOptions = {
  mode?: VercelPiCodingToolsMode;
  tools?: VercelPiCodingToolName[];
  cwd?: string;
  commandPrefix?: string;
};

export type RunVercelPiCodingAgentTurnInput = {
  prompt: string;
  env: VercelVirtualExecutionEnv;
  streamFn: StreamFn;
  systemPrompt?: string;
  messages?: PiAgentMessage[];
  tools?: VercelPiCodingToolName[];
  maxAssistantTurns?: number;
  model?: Model<Api>;
};

export type VercelPiCodingAgentTurnResult = {
  messages: PiAgentMessage[];
  events: PiAgentEvent[];
  snapshot: VirtualFileSnapshotEntry[];
};

const PI_CODING_AGENT_VERCEL_MODEL = {
  id: "learnordie-vercel-pi-coding-agent",
  name: "Learnordie Vercel Pi Coding Agent",
  api: "learnordie-ai-provider" as Api,
  provider: "learnordie",
  baseUrl: "",
  reasoning: false,
  input: ["text", "image"],
  cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
  contextWindow: 0,
  maxTokens: 0
} satisfies Model<Api>;

const EMPTY_USAGE: Usage = {
  input: 0,
  output: 0,
  cacheRead: 0,
  cacheWrite: 0,
  totalTokens: 0,
  cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 }
};

const grepSchema = Type.Object({
  pattern: Type.String({ description: "Search pattern (regex or literal string)" }),
  path: Type.Optional(Type.String({ description: "Directory or file to search (default: current directory)" })),
  glob: Type.Optional(Type.String({ description: "Filter files by glob pattern, e.g. '*.ts' or '**/*.spec.ts'" })),
  ignoreCase: Type.Optional(Type.Boolean({ description: "Case-insensitive search (default: false)" })),
  literal: Type.Optional(Type.Boolean({ description: "Treat pattern as literal string instead of regex (default: false)" })),
  context: Type.Optional(Type.Number({ description: "Number of lines to show before and after each match (default: 0)" })),
  limit: Type.Optional(Type.Number({ description: "Maximum number of matches to return (default: 100)" }))
});

export function createVercelPiCodingTools(
  env: VercelVirtualExecutionEnv,
  options: CreateVercelPiCodingToolsOptions = {}
): PiAgentTool[] {
  const cwd = normalizeVirtualPath(options.cwd ?? env.cwd);
  const operations = createVercelPiCodingOperations(env);
  const selectedTools = options.tools ?? defaultToolsForMode(options.mode ?? "all");
  return selectedTools.map((toolName): PiAgentTool => {
    switch (toolName) {
      case "read":
        return createReadTool(cwd, { autoResizeImages: false, operations: operations.read });
      case "bash":
        return createBashTool(cwd, {
          commandPrefix: options.commandPrefix,
          operations: operations.bash
        });
      case "edit":
        return createEditTool(cwd, { operations: operations.edit });
      case "write":
        return createWriteTool(cwd, { operations: operations.write });
      case "grep":
        return createVercelGrepTool(env, cwd);
      case "find":
        return createFindTool(cwd, { operations: operations.find });
      case "ls":
        return createLsTool(cwd, { operations: operations.ls });
    }
  });
}

export function createVercelPiCodingOperations(env: VercelVirtualExecutionEnv): {
  read: ReadOperations;
  bash: BashOperations;
  edit: EditOperations;
  write: WriteOperations;
  find: FindOperations;
  ls: LsOperations;
} {
  const readFileBuffer = async (absolutePath: string) => {
    const result = await env.readBinaryFile(absolutePath);
    if (!result.ok) throw toOperationError(result.error);
    return Buffer.from(result.value);
  };
  const accessFile = async (absolutePath: string) => {
    const result = await env.fileInfo(absolutePath);
    if (!result.ok) throw toOperationError(result.error);
    if (result.value.kind !== "file") throw new Error(`Not a file: ${absolutePath}`);
  };

  return {
    read: {
      readFile: readFileBuffer,
      access: accessFile,
      detectImageMimeType: async (absolutePath) => detectVirtualImageMimeType(env, absolutePath)
    },
    bash: {
      exec: async (command, cwd, options) => {
        const result = await env.exec(command, {
          cwd,
          timeout: options.timeout,
          abortSignal: options.signal,
          env: normalizeProcessEnv(options.env),
          onStdout: (chunk) => options.onData(Buffer.from(chunk)),
          onStderr: (chunk) => options.onData(Buffer.from(chunk))
        });
        if (!result.ok) {
          if (result.error.code === "aborted") throw new Error("aborted");
          if (result.error.code === "timeout") throw new Error(`timeout:${options.timeout ?? ""}`);
          throw result.error;
        }
        const exitCode = result.value.exitCode;
        return { exitCode };
      }
    },
    edit: {
      readFile: readFileBuffer,
      writeFile: async (absolutePath, content) => {
        const result = await env.writeFile(absolutePath, content);
        if (!result.ok) throw toOperationError(result.error);
      },
      access: accessFile
    },
    write: {
      writeFile: async (absolutePath, content) => {
        const result = await env.writeFile(absolutePath, content);
        if (!result.ok) throw toOperationError(result.error);
      },
      mkdir: async (dir) => {
        const result = await env.createDir(dir);
        if (!result.ok) throw toOperationError(result.error);
      }
    },
    find: {
      exists: async (absolutePath) => {
        const result = await env.exists(absolutePath);
        if (!result.ok) throw toOperationError(result.error);
        return result.value;
      },
      glob: (pattern, cwd, options) => virtualGlob(env, pattern, cwd, options)
    },
    ls: {
      exists: async (absolutePath) => {
        const result = await env.exists(absolutePath);
        if (!result.ok) throw toOperationError(result.error);
        return result.value;
      },
      stat: async (absolutePath) => {
        const result = await env.fileInfo(absolutePath);
        if (!result.ok) throw toOperationError(result.error);
        return {
          isDirectory: () => result.value.kind === "directory"
        };
      },
      readdir: async (absolutePath) => {
        const result = await env.listDir(absolutePath);
        if (!result.ok) throw toOperationError(result.error);
        return result.value.map((entry) => entry.name);
      }
    }
  };
}

export async function runVercelPiCodingAgentTurn(
  input: RunVercelPiCodingAgentTurnInput
): Promise<VercelPiCodingAgentTurnResult> {
  const events: PiAgentEvent[] = [];
  const tools = createVercelPiCodingTools(input.env, {
    tools: input.tools,
    mode: input.tools === undefined ? "all" : undefined
  });
  const context: PiAgentContext = {
    systemPrompt: input.systemPrompt ?? createVercelPiCodingSystemPrompt(),
    messages: input.messages ?? [],
    tools
  };
  const promptMessage: PiMessage = {
    role: "user",
    content: input.prompt,
    timestamp: Date.now()
  };
  let assistantTurns = 0;
  const maxAssistantTurns = input.maxAssistantTurns ?? 12;

  const messages = await runAgentLoop(
    [promptMessage],
    context,
    {
      model: input.model ?? PI_CODING_AGENT_VERCEL_MODEL,
      convertToLlm: (messages) => messages.filter(isPiMessage),
      toolExecution: "sequential",
      shouldStopAfterTurn: ({ message }) => {
        if (message.role === "assistant") assistantTurns += 1;
        if (assistantTurns >= maxAssistantTurns) return true;
        return message.role === "assistant" && !message.content.some((part) => part.type === "toolCall");
      }
    },
    (event) => {
      events.push(event);
    },
    undefined,
    input.streamFn
  );

  const snapshot = input.env.snapshotTextFiles(input.env.cwd);
  return {
    messages,
    events,
    snapshot: snapshot.ok ? snapshot.value : []
  };
}

export function createVercelPiCodingSystemPrompt(): string {
  return [
    "You are Pi coding-agent running inside a Vercel-compatible virtual execution environment.",
    "Use the provided Pi coding tools for file and shell work: read, bash, edit, write, grep, find, and ls.",
    "The filesystem is an isolated in-memory workspace. Do not claim to access the deployment host filesystem.",
    "The bash tool is a virtual allowlisted shell over that workspace, not a host process shell.",
    "Make code changes through write/edit and inspect files through read/grep/find/ls."
  ].join("\n");
}

function createVercelGrepTool(env: VercelVirtualExecutionEnv, cwd: string): PiAgentTool<typeof grepSchema, GrepToolDetails | undefined> {
  return {
    name: "grep",
    label: "grep",
    description: `Search file contents for a pattern. Returns matching lines with file paths and line numbers. Respects the virtual workspace. Output is truncated to 100 matches or ${DEFAULT_MAX_BYTES / 1024}KB (whichever is hit first).`,
    parameters: grepSchema,
    async execute(_toolCallId, { pattern, path: searchPathInput, glob, ignoreCase, literal, context, limit }, signal) {
      if (signal?.aborted) throw new Error("Operation aborted");
      const searchPath = resolveVirtualPath(searchPathInput || ".", cwd);
      const info = await env.fileInfo(searchPath, signal);
      if (!info.ok) throw new Error(`Path not found: ${searchPath}`);
      const files = info.value.kind === "directory"
        ? listVirtualFiles(env, searchPath).filter((filePath) => {
          const relative = path.relative(searchPath, filePath);
          return !isIgnoredPath(relative) && (!glob || matchesGlob(relative, glob));
        })
        : [searchPath];
      const matcher = createLineMatcher(pattern, { ignoreCase: !!ignoreCase, literal: !!literal });
      const contextLines = context && context > 0 ? context : 0;
      const effectiveLimit = Math.max(1, limit ?? 100);
      const outputLines: string[] = [];
      let matchCount = 0;
      let matchLimitReached: number | undefined;
      let linesTruncated = false;

      for (const filePath of files) {
        if (signal?.aborted) throw new Error("Operation aborted");
        const textResult = await env.readTextFile(filePath, signal);
        if (!textResult.ok) continue;
        const lines = splitLines(textResult.value);
        for (let index = 0; index < lines.length; index += 1) {
          if (!matcher(lines[index])) continue;
          matchCount += 1;
          if (matchCount > effectiveLimit) {
            matchLimitReached = effectiveLimit;
            break;
          }
          const start = contextLines > 0 ? Math.max(0, index - contextLines) : index;
          const end = contextLines > 0 ? Math.min(lines.length - 1, index + contextLines) : index;
          for (let current = start; current <= end; current += 1) {
            const { text, wasTruncated } = truncateLine(lines[current]);
            if (wasTruncated) linesTruncated = true;
            const displayPath = info.value.kind === "directory"
              ? path.relative(searchPath, filePath) || path.basename(filePath)
              : path.basename(filePath);
            outputLines.push(`${displayPath}${current === index ? ":" : "-"}${current + 1}${current === index ? ":" : "-"} ${text}`);
          }
        }
        if (matchLimitReached) break;
      }

      if (matchCount === 0) {
        return { content: [{ type: "text", text: "No matches found" }], details: undefined };
      }

      const rawOutput = outputLines.join("\n");
      const truncation = truncateHead(rawOutput, { maxLines: Number.MAX_SAFE_INTEGER });
      const notices: string[] = [];
      const details: GrepToolDetails = {};
      let output = truncation.content;
      if (matchLimitReached) {
        details.matchLimitReached = matchLimitReached;
        notices.push(`${effectiveLimit} matches limit reached. Use limit=${effectiveLimit * 2} for more, or refine pattern`);
      }
      if (truncation.truncated) {
        details.truncation = truncation;
        notices.push(`${formatSize(DEFAULT_MAX_BYTES)} limit reached`);
      }
      if (linesTruncated) {
        details.linesTruncated = true;
        notices.push("Some lines truncated. Use read tool to see full lines");
      }
      if (notices.length > 0) output += `\n\n[${notices.join(". ")}]`;
      return {
        content: [{ type: "text", text: output }],
        details: Object.keys(details).length > 0 ? details : undefined
      };
    }
  };
}

function defaultToolsForMode(mode: VercelPiCodingToolsMode): VercelPiCodingToolName[] {
  if (mode === "coding") return ["read", "bash", "edit", "write"];
  if (mode === "readOnly") return ["read", "grep", "find", "ls"];
  return [...vercelPiCodingToolNames];
}

function normalizeProcessEnv(env: NodeJS.ProcessEnv | undefined): Record<string, string> | undefined {
  if (!env) return undefined;
  return Object.fromEntries(Object.entries(env).filter((entry): entry is [string, string] => typeof entry[1] === "string"));
}

function toOperationError(error: Error & { code?: string }): Error & { code?: string } {
  const result = new Error(error.message) as Error & { code?: string };
  result.code = error.code;
  return result;
}

function normalizeVirtualPath(filePath: string): string {
  return path.normalize(filePath || "/").startsWith("/") ? path.normalize(filePath || "/") : `/${path.normalize(filePath || "/")}`;
}

function resolveVirtualPath(filePath: string, cwd: string): string {
  return path.isAbsolute(filePath) ? normalizeVirtualPath(filePath) : normalizeVirtualPath(path.resolve(cwd, filePath));
}

async function detectVirtualImageMimeType(env: VercelVirtualExecutionEnv, absolutePath: string): Promise<string | undefined> {
  const result = await env.readBinaryFile(absolutePath);
  if (!result.ok) return undefined;
  const bytes = result.value;
  if (bytes[0] === 0x89 && bytes[1] === 0x50 && bytes[2] === 0x4e && bytes[3] === 0x47) return "image/png";
  if (bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff) return "image/jpeg";
  if (bytes[0] === 0x47 && bytes[1] === 0x49 && bytes[2] === 0x46) return "image/gif";
  if (bytes[0] === 0x52 && bytes[1] === 0x49 && bytes[2] === 0x46 && bytes[3] === 0x46 && bytes[8] === 0x57 && bytes[9] === 0x45 && bytes[10] === 0x42 && bytes[11] === 0x50) return "image/webp";
  return undefined;
}

function virtualGlob(env: VercelVirtualExecutionEnv, pattern: string, cwd: string, options: { ignore: string[]; limit: number }): string[] {
  const files = listVirtualFiles(env, cwd);
  const results: string[] = [];
  for (const filePath of files) {
    const relative = path.relative(cwd, filePath);
    if (isIgnoredPath(relative, options.ignore)) continue;
    if (!matchesGlob(relative, pattern)) continue;
    results.push(filePath);
    if (results.length >= options.limit) break;
  }
  return results;
}

function listVirtualFiles(env: VercelVirtualExecutionEnv, rootPath: string): string[] {
  const snapshot = env.snapshotTextFiles(rootPath);
  if (!snapshot.ok) return [];
  return snapshot.value
    .filter((entry) => entry.kind === "file")
    .map((entry) => entry.path)
    .sort((left, right) => left.localeCompare(right));
}

function isIgnoredPath(relativePath: string, patterns = ["**/node_modules/**", "**/.git/**"]): boolean {
  // Vercel workspaces are app-seeded virtual trees, so the first port keeps a
  // fixed ignore set instead of reading host .gitignore files.
  return patterns.some((pattern) => matchesGlob(relativePath, pattern));
}

function matchesGlob(value: string, pattern: string): boolean {
  const normalizedValue = value.replace(/^\.\//, "");
  const normalizedPattern = pattern.replace(/^\.\//, "");
  return globToRegExp(normalizedPattern).test(normalizedValue);
}

function globToRegExp(pattern: string): RegExp {
  let source = "";
  for (let index = 0; index < pattern.length; index += 1) {
    const char = pattern[index];
    const next = pattern[index + 1];
    if (char === "*" && next === "*") {
      if (pattern[index + 2] === "/") {
        source += "(?:.*/)?";
        index += 2;
      } else {
        source += ".*";
        index += 1;
      }
    } else if (char === "*") {
      source += "[^/]*";
    } else if (char === "?") {
      source += "[^/]";
    } else {
      source += escapeRegExp(char);
    }
  }
  return new RegExp(`^${source}$`);
}

function createLineMatcher(pattern: string, options: { ignoreCase: boolean; literal: boolean }): (line: string) => boolean {
  if (options.literal) {
    const needle = options.ignoreCase ? pattern.toLowerCase() : pattern;
    return (line) => (options.ignoreCase ? line.toLowerCase() : line).includes(needle);
  }
  try {
    const regex = new RegExp(pattern, options.ignoreCase ? "i" : "");
    return (line) => regex.test(line);
  } catch {
    const needle = options.ignoreCase ? pattern.toLowerCase() : pattern;
    return (line) => (options.ignoreCase ? line.toLowerCase() : line).includes(needle);
  }
}

function splitLines(text: string): string[] {
  if (!text) return [];
  const lines = text.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n");
  if (text.endsWith("\n")) lines.pop();
  return lines;
}

function escapeRegExp(value: string): string {
  return value.replace(/[|\\{}()[\]^$+?.]/g, "\\$&");
}

function isPiMessage(message: PiAgentMessage): message is PiMessage {
  return message.role === "user" || message.role === "assistant" || message.role === "toolResult";
}

export function createVercelPiCodingToolCallMessage(
  name: VercelPiCodingToolName,
  args: Record<string, unknown>,
  id = `vercel-${name}-${Math.random().toString(36).slice(2, 8)}`
): AssistantMessage {
  return {
    role: "assistant",
    content: [{ type: "toolCall", id, name, arguments: args }],
    api: "learnordie-ai-provider" as Api,
    provider: "learnordie",
    model: PI_CODING_AGENT_VERCEL_MODEL.id,
    usage: EMPTY_USAGE,
    stopReason: "toolUse",
    timestamp: Date.now()
  };
}

export function createVercelPiCodingTextMessage(text: string): AssistantMessage {
  return {
    role: "assistant",
    content: [{ type: "text", text }],
    api: "learnordie-ai-provider" as Api,
    provider: "learnordie",
    model: PI_CODING_AGENT_VERCEL_MODEL.id,
    usage: EMPTY_USAGE,
    stopReason: "stop",
    timestamp: Date.now()
  };
}
