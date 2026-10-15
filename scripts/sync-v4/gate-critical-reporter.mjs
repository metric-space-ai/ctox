import { tap } from 'node:test/reporters';
import { resolve } from 'node:path';

// Node emits a passing file wrapper even when a name filter selects no tests.
// Only located, executed leaf cases prove that a named contract ran.
export default async function* criticalReporter(source) {
  async function* observe() {
    for await (const event of source) {
      const data = event.data;
      if (event.type === 'test:pass' && Number.isInteger(data.line)
          && data.details?.type !== 'suite' && !data.skip && !data.todo
          && (!data.file || resolve(data.name) !== resolve(data.file))) {
        yield { type: 'test:diagnostic', data: {
          message: `v4_case ${JSON.stringify({ name: data.name, file: data.file })}`,
          nesting: 0,
        } };
      }
      yield event;
    }
  }
  yield* tap(observe());
}
