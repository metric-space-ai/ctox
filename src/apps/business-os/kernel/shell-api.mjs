import { CtoxError } from './values.mjs';
export function createShellKernel(kernel) {
  if (!kernel?.shellApi) throw new CtoxError('invalid', 'createShellKernel braucht einen Kernel aus createKernel()');
  return kernel.shellApi();
}
