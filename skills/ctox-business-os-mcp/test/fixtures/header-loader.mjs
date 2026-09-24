// Offline fixture only. Never import a real credential loader from tests.
export async function loadBearerToken() {
  const scenario = process.env.CTOX_HEADER_HELPER_FIXTURE;
  const marker = 'offline-fixture-secret-NEVER-LOG';
  if (scenario === 'throw') throw new Error(marker);
  if (scenario === 'stdout') console.log(marker);
  if (scenario === 'stderr') console.error(marker);
  if (scenario === 'stdout-limit') process.stdout.write('x'.repeat(40000));
  if (scenario === 'stderr-limit') process.stderr.write('x'.repeat(10000));
  if (scenario === 'hang') await new Promise(() => setInterval(() => {}, 1000));
  if (scenario === 'invalid') return { value: marker };
  if (scenario === 'injection') return 'value\r\nX-Other: bad';
  return marker;
}
