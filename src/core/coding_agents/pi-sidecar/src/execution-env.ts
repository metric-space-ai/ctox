import { posix as path } from "node:path";
import { VirtualFS } from "virtualfs";
import {
  ExecutionError,
  FileError,
  err,
  ok,
  toError,
  type ExecutionEnv,
  type FileInfo,
  type Result,
  type ShellExecOptions
} from "@earendil-works/pi-agent-core";

type VirtualFsStats = ReturnType<VirtualFS["lstatSync"]>;

type ShellCommandResult = {
  stdout: string;
  stderr: string;
  exitCode: number;
};

type ShellRedirect = {
  path: string;
  append: boolean;
};

type ParsedShellCommand = {
  argv: string[];
  stdoutRedirect?: ShellRedirect;
  stderrRedirect?: ShellRedirect;
};

export type VercelVirtualExecutionEnvOptions = {
  cwd?: string;
  files?: Record<string, string | Uint8Array>;
  shellEnv?: Record<string, string>;
  maxOutputBytes?: number;
  maxOutputLines?: number;
};

export type VirtualFileSnapshotEntry = {
  path: string;
  kind: "file" | "directory";
  content?: string;
};

const DEFAULT_CWD = "/workspace";
const DEFAULT_MAX_OUTPUT_BYTES = 48 * 1024;
const DEFAULT_MAX_OUTPUT_LINES = 1_800;
const OUTPUT_TRUNCATION_NOTICE_BUDGET_BYTES = 512;
const OUTPUT_TRUNCATION_NOTICE_BUDGET_LINES = 4;
const TEXT_DECODER = new TextDecoder();

export class VercelVirtualExecutionEnv implements ExecutionEnv {
  readonly fs: VirtualFS;
  cwd: string;

  private readonly shellEnv: Record<string, string>;
  private readonly maxOutputBytes: number;
  private readonly maxOutputLines: number;

  constructor(options: VercelVirtualExecutionEnvOptions = {}) {
    this.fs = new VirtualFS();
    this.cwd = normalizeAbsolutePath(options.cwd ?? DEFAULT_CWD);
    this.shellEnv = options.shellEnv ?? {};
    this.maxOutputBytes = Math.min(options.maxOutputBytes ?? DEFAULT_MAX_OUTPUT_BYTES, DEFAULT_MAX_OUTPUT_BYTES);
    this.maxOutputLines = Math.min(options.maxOutputLines ?? DEFAULT_MAX_OUTPUT_LINES, DEFAULT_MAX_OUTPUT_LINES);

    this.fs.mkdirpSync(this.cwd);
    this.fs.mkdirpSync("/tmp");

    for (const [filePath, content] of Object.entries(options.files ?? {})) {
      const absolute = this.resolvePath(filePath);
      this.ensureParentDirSync(absolute);
      this.fs.writeFileSync(absolute, toVirtualFsContent(content));
    }
  }

  async absolutePath(filePath: string, abortSignal?: AbortSignal): Promise<Result<string, FileError>> {
    const resolved = this.resolvePath(filePath);
    const aborted = abortFileResult<string>(abortSignal, resolved);
    if (aborted) return aborted;
    return ok(resolved);
  }

  async joinPath(parts: string[], abortSignal?: AbortSignal): Promise<Result<string, FileError>> {
    const joined = parts.length > 0 ? path.join(...parts) : ".";
    const aborted = abortFileResult<string>(abortSignal, joined);
    if (aborted) return aborted;
    return ok(joined);
  }

  async readTextFile(filePath: string, abortSignal?: AbortSignal): Promise<Result<string, FileError>> {
    const resolved = this.resolvePath(filePath);
    const aborted = abortFileResult<string>(abortSignal, resolved);
    if (aborted) return aborted;
    try {
      return ok(String(this.fs.readFileSync(resolved, "utf8")));
    } catch (error) {
      return err(toFileError(error, resolved));
    }
  }

  async readTextLines(
    filePath: string,
    options?: { maxLines?: number; abortSignal?: AbortSignal }
  ): Promise<Result<string[], FileError>> {
    const resolved = this.resolvePath(filePath);
    const aborted = abortFileResult<string[]>(options?.abortSignal, resolved);
    if (aborted) return aborted;
    if (options?.maxLines !== undefined && options.maxLines <= 0) return ok([]);
    try {
      const lines = splitLines(String(this.fs.readFileSync(resolved, "utf8")));
      return ok(options?.maxLines === undefined ? lines : lines.slice(0, options.maxLines));
    } catch (error) {
      return err(toFileError(error, resolved));
    }
  }

  async readBinaryFile(filePath: string, abortSignal?: AbortSignal): Promise<Result<Uint8Array, FileError>> {
    const resolved = this.resolvePath(filePath);
    const aborted = abortFileResult<Uint8Array>(abortSignal, resolved);
    if (aborted) return aborted;
    try {
      return ok(toUint8Array(this.fs.readFileSync(resolved)));
    } catch (error) {
      return err(toFileError(error, resolved));
    }
  }

  async writeFile(filePath: string, content: string | Uint8Array, abortSignal?: AbortSignal): Promise<Result<void, FileError>> {
    const resolved = this.resolvePath(filePath);
    const aborted = abortFileResult<void>(abortSignal, resolved);
    if (aborted) return aborted;
    try {
      this.ensureParentDirSync(resolved);
      this.fs.writeFileSync(resolved, toVirtualFsContent(content));
      return ok(undefined);
    } catch (error) {
      return err(toFileError(error, resolved));
    }
  }

  async appendFile(filePath: string, content: string | Uint8Array, abortSignal?: AbortSignal): Promise<Result<void, FileError>> {
    const resolved = this.resolvePath(filePath);
    const aborted = abortFileResult<void>(abortSignal, resolved);
    if (aborted) return aborted;
    try {
      this.ensureParentDirSync(resolved);
      this.fs.appendFileSync(resolved, toVirtualFsContent(content));
      return ok(undefined);
    } catch (error) {
      return err(toFileError(error, resolved));
    }
  }

  async fileInfo(filePath: string, abortSignal?: AbortSignal): Promise<Result<FileInfo, FileError>> {
    const resolved = this.resolvePath(filePath);
    const aborted = abortFileResult<FileInfo>(abortSignal, resolved);
    if (aborted) return aborted;
    try {
      return fileInfoFromStats(resolved, this.fs.lstatSync(resolved));
    } catch (error) {
      return err(toFileError(error, resolved));
    }
  }

  async listDir(filePath: string, abortSignal?: AbortSignal): Promise<Result<FileInfo[], FileError>> {
    const resolved = this.resolvePath(filePath);
    const aborted = abortFileResult<FileInfo[]>(abortSignal, resolved);
    if (aborted) return aborted;
    try {
      const infos: FileInfo[] = [];
      for (const rawName of this.fs.readdirSync(resolved, "utf8")) {
        const name = String(rawName);
        const childPath = path.join(resolved, name);
        const info = fileInfoFromStats(childPath, this.fs.lstatSync(childPath));
        if (!info.ok) return info;
        infos.push(info.value);
      }
      return ok(infos.sort((a, b) => a.name.localeCompare(b.name)));
    } catch (error) {
      return err(toFileError(error, resolved));
    }
  }

  async canonicalPath(filePath: string, abortSignal?: AbortSignal): Promise<Result<string, FileError>> {
    const resolved = this.resolvePath(filePath);
    const aborted = abortFileResult<string>(abortSignal, resolved);
    if (aborted) return aborted;
    try {
      return ok(String(this.fs.realpathSync(resolved, "utf8")));
    } catch (error) {
      return err(toFileError(error, resolved));
    }
  }

  async exists(filePath: string, abortSignal?: AbortSignal): Promise<Result<boolean, FileError>> {
    const info = await this.fileInfo(filePath, abortSignal);
    if (info.ok) return ok(true);
    if (info.error.code === "not_found") return ok(false);
    return err(info.error);
  }

  async createDir(
    filePath: string,
    options?: { recursive?: boolean; abortSignal?: AbortSignal }
  ): Promise<Result<void, FileError>> {
    const resolved = this.resolvePath(filePath);
    const aborted = abortFileResult<void>(options?.abortSignal, resolved);
    if (aborted) return aborted;
    try {
      if (resolved === "/") return ok(undefined);
      if (options?.recursive ?? true) {
        this.fs.mkdirpSync(resolved);
      } else {
        this.fs.mkdirSync(resolved);
      }
      return ok(undefined);
    } catch (error) {
      return err(toFileError(error, resolved));
    }
  }

  async remove(
    filePath: string,
    options?: { recursive?: boolean; force?: boolean; abortSignal?: AbortSignal }
  ): Promise<Result<void, FileError>> {
    const resolved = this.resolvePath(filePath);
    const aborted = abortFileResult<void>(options?.abortSignal, resolved);
    if (aborted) return aborted;
    try {
      this.removeSync(resolved, {
        recursive: options?.recursive ?? false,
        force: options?.force ?? false
      });
      return ok(undefined);
    } catch (error) {
      const fileError = toFileError(error, resolved);
      if ((options?.force ?? false) && fileError.code === "not_found") return ok(undefined);
      return err(fileError);
    }
  }

  async createTempDir(prefix = "tmp-", abortSignal?: AbortSignal): Promise<Result<string, FileError>> {
    const aborted = abortFileResult<string>(abortSignal, "/tmp");
    if (aborted) return aborted;
    try {
      this.fs.mkdirpSync("/tmp");
      for (let attempt = 0; attempt < 50; attempt += 1) {
        const tempPath = path.join("/tmp", `${prefix}${randomId()}`);
        try {
          this.fs.mkdirSync(tempPath);
          return ok(tempPath);
        } catch (error) {
          if (getErrorCode(error) !== "EEXIST") throw error;
        }
      }
      return err(new FileError("unknown", "Could not allocate a unique virtual temp directory.", "/tmp"));
    } catch (error) {
      return err(toFileError(error, "/tmp"));
    }
  }

  async createTempFile(options?: {
    prefix?: string;
    suffix?: string;
    abortSignal?: AbortSignal;
  }): Promise<Result<string, FileError>> {
    const dir = await this.createTempDir("tmp-", options?.abortSignal);
    if (!dir.ok) return dir;
    const filePath = path.join(dir.value, `${options?.prefix ?? ""}${randomId()}${options?.suffix ?? ""}`);
    try {
      this.fs.writeFileSync(filePath, "");
      return ok(filePath);
    } catch (error) {
      return err(toFileError(error, filePath));
    }
  }

  async exec(command: string, options?: ShellExecOptions): Promise<Result<ShellCommandResult, ExecutionError>> {
    if (options?.abortSignal?.aborted) return err(new ExecutionError("aborted", "aborted"));
    const startedAt = Date.now();
    const timeoutMs = typeof options?.timeout === "number" ? options.timeout * 1000 : undefined;
    const cwd = this.resolvePath(options?.cwd ?? this.cwd);
    const cwdInfo = await this.fileInfo(cwd, options?.abortSignal);
    if (!cwdInfo.ok || cwdInfo.value.kind !== "directory") {
      return ok({
        stdout: "",
        stderr: `shell: ${cwd}: working directory not found\n`,
        exitCode: 1
      });
    }

    const parsed = parseShell(command);
    if (!parsed.ok) {
      return ok({ stdout: "", stderr: `shell: ${parsed.error}\n`, exitCode: 2 });
    }
    if (parsed.value.length === 0) {
      return ok({ stdout: "", stderr: "", exitCode: 0 });
    }

    let input = "";
    let stderr = "";
    let exitCode = 0;

    for (const parsedCommand of parsed.value) {
      const interrupted = this.getInterruptedExecution(options?.abortSignal, startedAt, timeoutMs);
      if (interrupted) return interrupted;

      const commandResult = this.executeParsedCommand(parsedCommand, input, cwd, {
        ...this.shellEnv,
        ...(options?.env ?? {})
      });

      const redirectResult = this.applyRedirects(parsedCommand, commandResult, cwd);
      if (!redirectResult.ok) {
        return ok(redirectResult.error);
      }
      stderr += redirectResult.value.stderr;
      input = redirectResult.value.stdout;
      exitCode = redirectResult.value.exitCode;
    }

    let stdout = input;
    const truncated = truncateShellOutput(stdout, stderr, this.maxOutputBytes, this.maxOutputLines);
    stdout = truncated.stdout;
    stderr = truncated.stderr;

    try {
      if (stdout) options?.onStdout?.(stdout);
      if (stderr) options?.onStderr?.(stderr);
    } catch (error) {
      const cause = toError(error);
      return err(new ExecutionError("callback_error", cause.message, cause));
    }

    const interrupted = this.getInterruptedExecution(options?.abortSignal, startedAt, timeoutMs);
    if (interrupted) return interrupted;
    return ok({ stdout, stderr, exitCode });
  }

  async cleanup(): Promise<void> {
    // A virtualfs instance is owned per ExecutionEnv and has no external handles.
  }

  snapshotTextFiles(rootPath = "/"): Result<VirtualFileSnapshotEntry[], FileError> {
    const resolved = this.resolvePath(rootPath);
    try {
      const entries: VirtualFileSnapshotEntry[] = [];
      this.walkSnapshot(resolved, entries);
      return ok(entries);
    } catch (error) {
      return err(toFileError(error, resolved));
    }
  }

  private resolvePath(filePath: string): string {
    return path.isAbsolute(filePath)
      ? normalizeAbsolutePath(filePath)
      : normalizeAbsolutePath(path.resolve(this.cwd, filePath));
  }

  private resolveFrom(cwd: string, filePath: string): string {
    return path.isAbsolute(filePath)
      ? normalizeAbsolutePath(filePath)
      : normalizeAbsolutePath(path.resolve(cwd, filePath));
  }

  private ensureParentDirSync(filePath: string): void {
    const parent = path.dirname(filePath);
    if (parent && parent !== "." && parent !== "/") {
      this.fs.mkdirpSync(parent);
    }
  }

  private getInterruptedExecution(
    abortSignal: AbortSignal | undefined,
    startedAt: number,
    timeoutMs: number | undefined
  ): Result<ShellCommandResult, ExecutionError> | undefined {
    if (abortSignal?.aborted) return err(new ExecutionError("aborted", "aborted"));
    if (timeoutMs !== undefined && Date.now() - startedAt > timeoutMs) {
      return err(new ExecutionError("timeout", `timeout:${timeoutMs / 1000}`));
    }
    return undefined;
  }

  private executeParsedCommand(
    parsedCommand: ParsedShellCommand,
    stdin: string,
    cwd: string,
    shellEnv: Record<string, string>
  ): ShellCommandResult {
    const [name, ...args] = parsedCommand.argv;
    switch (name) {
      case "pwd":
        return this.execPwd(args, cwd);
      case "echo":
        return this.execEcho(args);
      case "ls":
        return this.execLs(args, cwd);
      case "cat":
        return this.execCat(args, stdin, cwd);
      case "head":
        return this.execHeadTail("head", args, stdin, cwd);
      case "tail":
        return this.execHeadTail("tail", args, stdin, cwd);
      case "wc":
        return this.execWc(args, stdin, cwd);
      case "grep":
        return this.execGrep(args, stdin, cwd);
      case "cp":
        return this.execCp(args, cwd);
      case "mv":
        return this.execMv(args, cwd);
      case "rm":
        return this.execRm(args, cwd);
      case "mkdir":
        return this.execMkdir(args, cwd);
      case "touch":
        return this.execTouch(args, cwd);
      case "diff":
        return this.execDiff(args, cwd);
      case "printenv":
        return this.execPrintenv(args, shellEnv);
      default:
        return {
          stdout: "",
          stderr: `${name}: command not available in Vercel virtual shell\n`,
          exitCode: 127
        };
    }
  }

  private applyRedirects(
    parsedCommand: ParsedShellCommand,
    result: ShellCommandResult,
    cwd: string
  ): Result<ShellCommandResult, ShellCommandResult> {
    try {
      let stdout = result.stdout;
      let stderr = result.stderr;
      if (parsedCommand.stdoutRedirect) {
        this.writeShellRedirect(parsedCommand.stdoutRedirect, stdout, cwd);
        stdout = "";
      }
      if (parsedCommand.stderrRedirect) {
        this.writeShellRedirect(parsedCommand.stderrRedirect, stderr, cwd);
        stderr = "";
      }
      return ok({ stdout, stderr, exitCode: result.exitCode });
    } catch (error) {
      const message = toFileError(error).message;
      return err({
        stdout: "",
        stderr: `shell: redirect failed: ${message}\n`,
        exitCode: 1
      });
    }
  }

  private writeShellRedirect(redirect: ShellRedirect, content: string, cwd: string): void {
    const target = this.resolveFrom(cwd, redirect.path);
    this.ensureParentDirSync(target);
    if (redirect.append) {
      this.fs.appendFileSync(target, content);
    } else {
      this.fs.writeFileSync(target, content);
    }
  }

  private execPwd(args: string[], cwd: string): ShellCommandResult {
    if (args.length > 0) return shellUsage("pwd", "does not accept arguments");
    return { stdout: `${cwd}\n`, stderr: "", exitCode: 0 };
  }

  private execEcho(args: string[]): ShellCommandResult {
    const noNewline = args[0] === "-n";
    const words = noNewline ? args.slice(1) : args;
    return {
      stdout: `${words.join(" ")}${noNewline ? "" : "\n"}`,
      stderr: "",
      exitCode: 0
    };
  }

  private execLs(args: string[], cwd: string): ShellCommandResult {
    const options = new Set<string>();
    const targets: string[] = [];
    for (const arg of args) {
      if (arg.startsWith("-")) {
        for (const option of arg.slice(1)) options.add(option);
      } else {
        targets.push(arg);
      }
    }
    const paths = targets.length > 0 ? targets : ["."];
    let stdout = "";
    let stderr = "";
    let exitCode = 0;

    for (const item of paths) {
      const resolved = this.resolveFrom(cwd, item);
      try {
        const stats = this.fs.lstatSync(resolved);
        if (stats.isDirectory()) {
          const names = this.fs.readdirSync(resolved, "utf8")
            .map(String)
            .filter((name) => options.has("a") || !name.startsWith("."))
            .sort((a, b) => a.localeCompare(b));
          stdout += names.join("\n");
          if (names.length > 0) stdout += "\n";
        } else {
          stdout += `${item}\n`;
        }
      } catch (error) {
        stderr += formatShellFileError("ls", resolved, error);
        exitCode = 1;
      }
    }

    return { stdout, stderr, exitCode };
  }

  private execCat(args: string[], stdin: string, cwd: string): ShellCommandResult {
    if (args.length === 0) return { stdout: stdin, stderr: "", exitCode: 0 };
    let stdout = "";
    let stderr = "";
    let exitCode = 0;
    for (const arg of args) {
      const resolved = this.resolveFrom(cwd, arg);
      try {
        stdout += String(this.fs.readFileSync(resolved, "utf8"));
      } catch (error) {
        stderr += formatShellFileError("cat", resolved, error);
        exitCode = 1;
      }
    }
    return { stdout, stderr, exitCode };
  }

  private execHeadTail(kind: "head" | "tail", args: string[], stdin: string, cwd: string): ShellCommandResult {
    const parsed = parseLineLimitArgs(args, 10);
    if (!parsed.ok) return shellUsage(kind, parsed.error);
    const linesFrom = (text: string) => splitLines(text);
    const select = (lines: string[]) => (
      kind === "head" ? lines.slice(0, parsed.value.limit) : lines.slice(Math.max(0, lines.length - parsed.value.limit))
    );

    if (parsed.value.paths.length === 0) {
      const selected = select(linesFrom(stdin));
      return { stdout: selected.join("\n") + (selected.length > 0 ? "\n" : ""), stderr: "", exitCode: 0 };
    }

    let stdout = "";
    let stderr = "";
    let exitCode = 0;
    for (const item of parsed.value.paths) {
      const resolved = this.resolveFrom(cwd, item);
      try {
        const selected = select(linesFrom(String(this.fs.readFileSync(resolved, "utf8"))));
        stdout += selected.join("\n") + (selected.length > 0 ? "\n" : "");
      } catch (error) {
        stderr += formatShellFileError(kind, resolved, error);
        exitCode = 1;
      }
    }
    return { stdout, stderr, exitCode };
  }

  private execWc(args: string[], stdin: string, cwd: string): ShellCommandResult {
    const options = new Set<string>();
    const files: string[] = [];
    for (const arg of args) {
      if (arg.startsWith("-")) {
        for (const option of arg.slice(1)) options.add(option);
      } else {
        files.push(arg);
      }
    }
    if (options.size === 0) {
      options.add("l");
      options.add("w");
      options.add("c");
    }
    const countsFor = (text: string, label?: string) => {
      const values: string[] = [];
      if (options.has("l")) values.push(String((text.match(/\n/g) ?? []).length));
      if (options.has("w")) values.push(String(text.trim() ? text.trim().split(/\s+/).length : 0));
      if (options.has("c")) values.push(String(Buffer.byteLength(text, "utf8")));
      return `${values.join(" ")}${label ? ` ${label}` : ""}\n`;
    };

    if (files.length === 0) return { stdout: countsFor(stdin), stderr: "", exitCode: 0 };

    let stdout = "";
    let stderr = "";
    let exitCode = 0;
    for (const file of files) {
      const resolved = this.resolveFrom(cwd, file);
      try {
        stdout += countsFor(String(this.fs.readFileSync(resolved, "utf8")), file);
      } catch (error) {
        stderr += formatShellFileError("wc", resolved, error);
        exitCode = 1;
      }
    }
    return { stdout, stderr, exitCode };
  }

  private execGrep(args: string[], stdin: string, cwd: string): ShellCommandResult {
    const options = new Set<string>();
    const rest: string[] = [];
    for (const arg of args) {
      if (arg.startsWith("-") && rest.length === 0) {
        for (const option of arg.slice(1)) options.add(option);
      } else {
        rest.push(arg);
      }
    }
    const [pattern, ...files] = rest;
    if (!pattern) return shellUsage("grep", "missing pattern");

    const matcher = createMatcher(pattern, {
      ignoreCase: options.has("i"),
      fixed: options.has("F"),
      invert: options.has("v")
    });
    const readInputs: Array<{ label?: string; text: string }> = [];
    let stderr = "";
    let exitCode = 0;

    if (files.length === 0) {
      readInputs.push({ text: stdin });
    } else {
      for (const file of files) {
        const resolved = this.resolveFrom(cwd, file);
        try {
          readInputs.push({ label: files.length > 1 ? file : undefined, text: String(this.fs.readFileSync(resolved, "utf8")) });
        } catch (error) {
          stderr += formatShellFileError("grep", resolved, error);
          exitCode = 2;
        }
      }
    }

    let stdout = "";
    for (const input of readInputs) {
      const lines = splitLines(input.text);
      lines.forEach((line, index) => {
        if (!matcher(line)) return;
        const prefixes = [
          input.label,
          options.has("n") ? String(index + 1) : undefined
        ].filter(Boolean);
        stdout += `${prefixes.length > 0 ? `${prefixes.join(":")}:` : ""}${line}\n`;
      });
    }
    if (!stdout && exitCode === 0) exitCode = 1;
    return { stdout, stderr, exitCode };
  }

  private execCp(args: string[], cwd: string): ShellCommandResult {
    const recursive = args.includes("-r") || args.includes("-R");
    const paths = args.filter((arg) => !arg.startsWith("-"));
    if (paths.length !== 2) return shellUsage("cp", "expected SOURCE DEST");
    const source = this.resolveFrom(cwd, paths[0]);
    let destination = this.resolveFrom(cwd, paths[1]);
    try {
      const destinationInfo = this.tryStats(destination);
      if (destinationInfo?.isDirectory()) destination = path.join(destination, path.basename(source));
      this.copySync(source, destination, recursive);
      return { stdout: "", stderr: "", exitCode: 0 };
    } catch (error) {
      return { stdout: "", stderr: formatShellFileError("cp", source, error), exitCode: 1 };
    }
  }

  private execMv(args: string[], cwd: string): ShellCommandResult {
    const paths = args.filter((arg) => !arg.startsWith("-"));
    if (paths.length !== 2) return shellUsage("mv", "expected SOURCE DEST");
    const source = this.resolveFrom(cwd, paths[0]);
    let destination = this.resolveFrom(cwd, paths[1]);
    try {
      const destinationInfo = this.tryStats(destination);
      if (destinationInfo?.isDirectory()) destination = path.join(destination, path.basename(source));
      this.ensureParentDirSync(destination);
      this.fs.renameSync(source, destination);
      return { stdout: "", stderr: "", exitCode: 0 };
    } catch (error) {
      return { stdout: "", stderr: formatShellFileError("mv", source, error), exitCode: 1 };
    }
  }

  private execRm(args: string[], cwd: string): ShellCommandResult {
    const recursive = args.some((arg) => arg.startsWith("-") && /r|R/.test(arg));
    const force = args.some((arg) => arg.startsWith("-") && arg.includes("f"));
    const targets = args.filter((arg) => !arg.startsWith("-"));
    if (targets.length === 0) return shellUsage("rm", "missing operand");
    let stderr = "";
    let exitCode = 0;
    for (const target of targets) {
      const resolved = this.resolveFrom(cwd, target);
      try {
        this.removeSync(resolved, { recursive, force });
      } catch (error) {
        const fileError = toFileError(error, resolved);
        if (force && fileError.code === "not_found") continue;
        stderr += formatShellFileError("rm", resolved, fileError);
        exitCode = 1;
      }
    }
    return { stdout: "", stderr, exitCode };
  }

  private execMkdir(args: string[], cwd: string): ShellCommandResult {
    const recursive = args.includes("-p");
    const dirs = args.filter((arg) => !arg.startsWith("-"));
    if (dirs.length === 0) return shellUsage("mkdir", "missing operand");
    let stderr = "";
    let exitCode = 0;
    for (const dir of dirs) {
      const resolved = this.resolveFrom(cwd, dir);
      try {
        if (recursive) this.fs.mkdirpSync(resolved);
        else this.fs.mkdirSync(resolved);
      } catch (error) {
        stderr += formatShellFileError("mkdir", resolved, error);
        exitCode = 1;
      }
    }
    return { stdout: "", stderr, exitCode };
  }

  private execTouch(args: string[], cwd: string): ShellCommandResult {
    const files = args.filter((arg) => !arg.startsWith("-"));
    if (files.length === 0) return shellUsage("touch", "missing operand");
    let stderr = "";
    let exitCode = 0;
    for (const file of files) {
      const resolved = this.resolveFrom(cwd, file);
      try {
        const existing = this.tryStats(resolved);
        if (existing?.isDirectory()) continue;
        this.ensureParentDirSync(resolved);
        const content = existing ? this.fs.readFileSync(resolved) : "";
        this.fs.writeFileSync(resolved, content);
      } catch (error) {
        stderr += formatShellFileError("touch", resolved, error);
        exitCode = 1;
      }
    }
    return { stdout: "", stderr, exitCode };
  }

  private execDiff(args: string[], cwd: string): ShellCommandResult {
    const files = args.filter((arg) => !arg.startsWith("-"));
    if (files.length !== 2) return shellUsage("diff", "expected FILE_A FILE_B");
    const left = this.resolveFrom(cwd, files[0]);
    const right = this.resolveFrom(cwd, files[1]);
    try {
      const leftText = String(this.fs.readFileSync(left, "utf8"));
      const rightText = String(this.fs.readFileSync(right, "utf8"));
      if (leftText === rightText) return { stdout: "", stderr: "", exitCode: 0 };
      return {
        stdout: createSimpleUnifiedDiff(files[0], leftText, files[1], rightText),
        stderr: "",
        exitCode: 1
      };
    } catch (error) {
      return { stdout: "", stderr: formatShellFileError("diff", left, error), exitCode: 2 };
    }
  }

  private execPrintenv(args: string[], shellEnv: Record<string, string>): ShellCommandResult {
    if (args.length === 0) {
      const stdout = Object.entries(shellEnv)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([name, value]) => `${name}=${value}`)
        .join("\n");
      return { stdout: stdout ? `${stdout}\n` : "", stderr: "", exitCode: 0 };
    }
    let stdout = "";
    let exitCode = 0;
    for (const name of args) {
      if (shellEnv[name] === undefined) {
        exitCode = 1;
      } else {
        stdout += `${shellEnv[name]}\n`;
      }
    }
    return { stdout, stderr: "", exitCode };
  }

  private tryStats(filePath: string): VirtualFsStats | undefined {
    try {
      return this.fs.lstatSync(filePath);
    } catch {
      return undefined;
    }
  }

  private copySync(source: string, destination: string, recursive: boolean): void {
    const sourceStats = this.fs.lstatSync(source);
    if (sourceStats.isDirectory()) {
      if (!recursive) throw new FileError("is_directory", "Source is a directory.", source);
      this.fs.mkdirpSync(destination);
      for (const rawName of this.fs.readdirSync(source, "utf8")) {
        const name = String(rawName);
        this.copySync(path.join(source, name), path.join(destination, name), recursive);
      }
      return;
    }
    this.ensureParentDirSync(destination);
    this.fs.writeFileSync(destination, this.fs.readFileSync(source));
  }

  private removeSync(filePath: string, options: { recursive: boolean; force: boolean }): void {
    if (filePath === "/") throw new FileError("permission_denied", "Refusing to remove virtual filesystem root.", filePath);
    let stats: VirtualFsStats;
    try {
      stats = this.fs.lstatSync(filePath);
    } catch (error) {
      if (options.force && getErrorCode(error) === "ENOENT") return;
      throw error;
    }
    if (stats.isDirectory()) {
      if (!options.recursive) throw new FileError("is_directory", "Path is a directory.", filePath);
      for (const rawName of this.fs.readdirSync(filePath, "utf8")) {
        this.removeSync(path.join(filePath, String(rawName)), options);
      }
      this.fs.rmdirSync(filePath);
      return;
    }
    this.fs.unlinkSync(filePath);
  }

  private walkSnapshot(filePath: string, entries: VirtualFileSnapshotEntry[]): void {
    const stats = this.fs.lstatSync(filePath);
    if (stats.isDirectory()) {
      entries.push({ path: filePath, kind: "directory" });
      for (const rawName of this.fs.readdirSync(filePath, "utf8").map(String).sort((a, b) => a.localeCompare(b))) {
        this.walkSnapshot(path.join(filePath, rawName), entries);
      }
      return;
    }
    if (stats.isFile()) {
      entries.push({
        path: filePath,
        kind: "file",
        content: String(this.fs.readFileSync(filePath, "utf8"))
      });
    }
  }
}

export function createVercelVirtualExecutionEnv(options?: VercelVirtualExecutionEnvOptions): VercelVirtualExecutionEnv {
  return new VercelVirtualExecutionEnv(options);
}

function normalizeAbsolutePath(filePath: string): string {
  const normalized = path.normalize(filePath || "/");
  return normalized.startsWith("/") ? normalized : `/${normalized}`;
}

function abortFileResult<T>(abortSignal: AbortSignal | undefined, filePath: string): Result<T, FileError> | undefined {
  return abortSignal?.aborted ? err(new FileError("aborted", "aborted", filePath)) : undefined;
}

function toVirtualFsContent(content: string | Uint8Array): string | Buffer {
  return typeof content === "string" ? content : Buffer.from(content);
}

function toUint8Array(content: string | Buffer): Uint8Array {
  return typeof content === "string" ? new TextEncoder().encode(content) : new Uint8Array(content);
}

function splitLines(text: string): string[] {
  if (!text) return [];
  const lines = text.split(/\r?\n/);
  if (text.endsWith("\n")) lines.pop();
  return lines;
}

function fileInfoFromStats(filePath: string, stats: VirtualFsStats): Result<FileInfo, FileError> {
  const kind = stats.isFile() ? "file" : stats.isDirectory() ? "directory" : stats.isSymbolicLink() ? "symlink" : undefined;
  if (!kind) return err(new FileError("invalid", "Unsupported virtual filesystem object.", filePath));
  return ok({
    name: path.basename(filePath),
    path: filePath,
    kind,
    size: stats.size,
    mtimeMs: stats.mtimeMs ?? stats.mtime?.getTime?.() ?? Date.now()
  });
}

function toFileError(error: unknown, filePath?: string): FileError {
  if (error instanceof FileError) return error;
  const cause = toError(error);
  switch (getErrorCode(error)) {
    case "ABORT_ERR":
      return new FileError("aborted", cause.message, filePath, cause);
    case "ENOENT":
      return new FileError("not_found", cause.message, filePath, cause);
    case "EACCES":
    case "EPERM":
      return new FileError("permission_denied", cause.message, filePath, cause);
    case "ENOTDIR":
      return new FileError("not_directory", cause.message, filePath, cause);
    case "EISDIR":
      return new FileError("is_directory", cause.message, filePath, cause);
    case "ENOSYS":
    case "ENOTSUP":
      return new FileError("not_supported", cause.message, filePath, cause);
    case "EINVAL":
    case "EEXIST":
    case "ENOTEMPTY":
    case "EBADF":
      return new FileError("invalid", cause.message, filePath, cause);
    default:
      return new FileError("unknown", cause.message, filePath, cause);
  }
}

function getErrorCode(error: unknown): string | undefined {
  return error instanceof Error && "code" in error ? String(error.code) : undefined;
}

function tokenizeShell(command: string): Result<string[], string> {
  const tokens: string[] = [];
  let current = "";
  let quote: "'" | "\"" | undefined;

  const pushCurrent = () => {
    if (current.length > 0) {
      tokens.push(current);
      current = "";
    }
  };

  for (let index = 0; index < command.length; index += 1) {
    const char = command[index];
    if (quote) {
      if (char === quote) {
        quote = undefined;
      } else if (char === "\\" && quote === "\"" && index + 1 < command.length) {
        index += 1;
        current += command[index];
      } else {
        current += char;
      }
      continue;
    }

    if (char === "'" || char === "\"") {
      quote = char;
      continue;
    }
    if (char === "\\" && index + 1 < command.length) {
      index += 1;
      current += command[index];
      continue;
    }
    if (/\s/.test(char)) {
      pushCurrent();
      continue;
    }
    if (char === "|") {
      pushCurrent();
      if (command[index + 1] === "|") {
        tokens.push("||");
        index += 1;
      } else {
        tokens.push("|");
      }
      continue;
    }
    if (char === "&" && command[index + 1] === "&") {
      pushCurrent();
      tokens.push("&&");
      index += 1;
      continue;
    }
    if (char === ";") {
      pushCurrent();
      tokens.push(";");
      continue;
    }
    if (char === ">") {
      if (current === "2") {
        current = "";
        tokens.push(command[index + 1] === ">" ? "2>>" : "2>");
      } else {
        pushCurrent();
        tokens.push(command[index + 1] === ">" ? ">>" : ">");
      }
      if (command[index + 1] === ">") index += 1;
      continue;
    }
    if (char === "<") return err("input redirection is not supported");
    current += char;
  }

  if (quote) return err("unterminated quote");
  pushCurrent();
  return ok(tokens);
}

function parseShell(command: string): Result<ParsedShellCommand[], string> {
  const tokenized = tokenizeShell(command.trim());
  if (!tokenized.ok) return tokenized;
  const tokens = tokenized.value;
  if (tokens.length === 0) return ok([]);
  if (tokens.some((token) => token === "&&" || token === "||" || token === ";")) {
    return err("command chaining is not supported");
  }

  const commands: ParsedShellCommand[] = [];
  let segment: string[] = [];
  const pushSegment = (): Result<void, string> => {
    const parsed = parseShellSegment(segment);
    if (!parsed.ok) return parsed;
    commands.push(parsed.value);
    segment = [];
    return ok(undefined);
  };

  for (const token of tokens) {
    if (token === "|") {
      if (segment.length === 0) return err("empty pipeline segment");
      const pushed = pushSegment();
      if (!pushed.ok) return pushed;
    } else {
      segment.push(token);
    }
  }
  if (segment.length === 0) return err("empty pipeline segment");
  const pushed = pushSegment();
  if (!pushed.ok) return pushed;
  return ok(commands);
}

function parseShellSegment(tokens: string[]): Result<ParsedShellCommand, string> {
  const argv: string[] = [];
  let stdoutRedirect: ShellRedirect | undefined;
  let stderrRedirect: ShellRedirect | undefined;
  for (let index = 0; index < tokens.length; index += 1) {
    const token = tokens[index];
    if (token === ">" || token === ">>" || token === "2>" || token === "2>>") {
      const target = tokens[index + 1];
      if (!target) return err(`missing redirect target after ${token}`);
      const redirect = { path: target, append: token.endsWith(">>") };
      if (token.startsWith("2")) stderrRedirect = redirect;
      else stdoutRedirect = redirect;
      index += 1;
    } else {
      argv.push(token);
    }
  }
  if (argv.length === 0) return err("empty command");
  return ok({ argv, stdoutRedirect, stderrRedirect });
}

function shellUsage(command: string, message: string): ShellCommandResult {
  return { stdout: "", stderr: `${command}: ${message}\n`, exitCode: 2 };
}

function parseLineLimitArgs(args: string[], defaultLimit: number): Result<{ limit: number; paths: string[] }, string> {
  let limit = defaultLimit;
  const paths: string[] = [];
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "-n") {
      const rawLimit = args[index + 1];
      if (!rawLimit) return err("missing line count");
      limit = Number.parseInt(rawLimit, 10);
      index += 1;
    } else if (arg.startsWith("-n")) {
      limit = Number.parseInt(arg.slice(2), 10);
    } else if (/^-\d+$/.test(arg)) {
      limit = Number.parseInt(arg.slice(1), 10);
    } else {
      paths.push(arg);
    }
  }
  if (!Number.isFinite(limit) || limit < 0) return err("invalid line count");
  return ok({ limit, paths });
}

function createMatcher(
  pattern: string,
  options: { ignoreCase: boolean; fixed: boolean; invert: boolean }
): (line: string) => boolean {
  let matches: (line: string) => boolean;
  if (options.fixed) {
    const needle = options.ignoreCase ? pattern.toLowerCase() : pattern;
    matches = (line) => (options.ignoreCase ? line.toLowerCase() : line).includes(needle);
  } else {
    try {
      const regex = new RegExp(pattern, options.ignoreCase ? "i" : "");
      matches = (line) => regex.test(line);
    } catch {
      const needle = options.ignoreCase ? pattern.toLowerCase() : pattern;
      matches = (line) => (options.ignoreCase ? line.toLowerCase() : line).includes(needle);
    }
  }
  return options.invert ? (line) => !matches(line) : matches;
}

function formatShellFileError(command: string, filePath: string, error: unknown): string {
  const fileError = error instanceof FileError ? error : toFileError(error, filePath);
  return `${command}: ${filePath}: ${fileError.message}\n`;
}

function createSimpleUnifiedDiff(leftName: string, leftText: string, rightName: string, rightText: string): string {
  const leftLines = splitLines(leftText);
  const rightLines = splitLines(rightText);
  return [
    `--- ${leftName}`,
    `+++ ${rightName}`,
    "@@",
    ...leftLines.map((line) => `-${line}`),
    ...rightLines.map((line) => `+${line}`)
  ].join("\n") + "\n";
}

function truncateShellOutput(
  stdout: string,
  stderr: string,
  maxBytes: number,
  maxLines: number
): { stdout: string; stderr: string } {
  const totalBytes = Buffer.byteLength(stdout, "utf8") + Buffer.byteLength(stderr, "utf8");
  const totalLines = countOutputLines(stdout) + countOutputLines(stderr);
  if (totalBytes <= maxBytes && totalLines <= maxLines) return { stdout, stderr };

  const byteBudget = Math.max(0, maxBytes - OUTPUT_TRUNCATION_NOTICE_BUDGET_BYTES);
  const lineBudget = Math.max(0, maxLines - OUTPUT_TRUNCATION_NOTICE_BUDGET_LINES);
  const stdoutSlice = takeOutputHead(stdout, byteBudget, lineBudget);
  const remainingBytes = Math.max(0, byteBudget - Buffer.byteLength(stdoutSlice.text, "utf8"));
  const remainingLines = Math.max(0, lineBudget - countOutputLines(stdoutSlice.text));
  const stderrSlice = takeOutputHead(stderr, remainingBytes, remainingLines);

  return {
    stdout: appendTruncationNotice(stdoutSlice.text, stdoutSlice.truncated, "stdout"),
    stderr: appendTruncationNotice(stderrSlice.text, stderrSlice.truncated, "stderr")
  };
}

function takeOutputHead(text: string, maxBytes: number, maxLines: number): { text: string; truncated: boolean } {
  if (!text) return { text, truncated: false };
  if (maxBytes <= 0 || maxLines <= 0) return { text: "", truncated: true };
  if (Buffer.byteLength(text, "utf8") <= maxBytes && countOutputLines(text) <= maxLines) {
    return { text, truncated: false };
  }

  let output = "";
  let outputBytes = 0;
  let outputLines = 0;
  const parts = text.split(/(\n)/);

  for (const part of parts) {
    if (!part) continue;
    const partLines = part === "\n" ? 1 : 0;
    const partBytes = Buffer.byteLength(part, "utf8");
    if (outputLines + partLines > maxLines || outputBytes + partBytes > maxBytes) {
      if (outputBytes === 0 && part !== "\n") {
        return {
          text: TEXT_DECODER.decode(Buffer.from(part).subarray(0, maxBytes)),
          truncated: true
        };
      }
      return { text: output, truncated: true };
    }
    output += part;
    outputBytes += partBytes;
    outputLines += partLines;
  }

  return { text: output, truncated: output !== text };
}

function appendTruncationNotice(text: string, truncated: boolean, streamName: "stdout" | "stderr"): string {
  if (!truncated) return text;
  const prefix = text && !text.endsWith("\n") ? "\n" : "";
  return `${text}${prefix}[virtual shell ${streamName} truncated]\n`;
}

function countOutputLines(text: string): number {
  if (!text) return 0;
  const newlines = text.match(/\n/g)?.length ?? 0;
  return text.endsWith("\n") ? newlines : newlines + 1;
}

function randomId(): string {
  return Math.random().toString(36).slice(2, 10);
}
