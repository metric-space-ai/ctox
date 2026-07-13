import { createCtoxForkRuntime } from './ctox-fork-core.mjs';

export function createOfficeFrameRuntime(options) {
  return createCtoxForkRuntime({ ...options, kind: 'document' });
}
