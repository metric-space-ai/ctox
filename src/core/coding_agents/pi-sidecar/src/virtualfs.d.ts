declare module "virtualfs" {
  export type VirtualFsStats = {
    size: number;
    mtime?: Date;
    mtimeMs?: number;
    isFile(): boolean;
    isDirectory(): boolean;
    isSymbolicLink(): boolean;
  };

  export class VirtualFS {
    constructor();
    getCwd(): string;
    chdir(path: string): void;
    accessSync(path: string): void;
    appendFileSync(path: string | number, data?: string | Uint8Array | Buffer, options?: string | { encoding?: string | null; flag?: string }): void;
    copyFileSync(srcPath: string, dstPath: string, flags?: number): void;
    lstatSync(path: string): VirtualFsStats;
    mkdirSync(path: string, mode?: number): void;
    mkdirpSync(path: string, mode?: number): void;
    readFileSync(path: string | number, options?: string | { encoding?: string | null; flag?: string }): string | Buffer;
    readdirSync(path: string, options?: string | { encoding?: string | null }): Array<string | Buffer>;
    realpathSync(path: string, options?: string | { encoding?: string | null }): string | Buffer;
    renameSync(oldPath: string, newPath: string): void;
    rmdirSync(path: string): void;
    statSync(path: string): VirtualFsStats;
    unlinkSync(path: string): void;
    writeFileSync(path: string | number, data?: string | Uint8Array | Buffer, options?: string | { encoding?: string | null; flag?: string }): void;
  }

  const virtualfs: VirtualFS;
  export default virtualfs;
}
