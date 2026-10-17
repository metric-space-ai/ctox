// Development/test entry only; excluded from the production slot.
export * from './index';
import type {StubKernelOptions, StubKernel, KernelTransport} from './index';
export declare function createStubKernel(opts?: StubKernelOptions): StubKernel;
export declare const MemoryCoreTransport: new(options?: Record<string, unknown>) => KernelTransport & {readonly kind:'memory'};
