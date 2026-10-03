const { createInterface } = require('node:readline');

// Pipe chunks are arbitrary byte ranges, not log records. Prefix complete
// decoded lines so split JSON and UTF-8 remain intact in CI evidence.
function forwardNativeLogLines(input, output, prefix, onLine = () => {}) {
  const lines = createInterface({ input, crlfDelay: Infinity, terminal: false });
  lines.on('line', (line) => {
    output.write(`${prefix}${line}\n`);
    onLine(line);
  });
  return lines;
}

module.exports = { forwardNativeLogLines };
