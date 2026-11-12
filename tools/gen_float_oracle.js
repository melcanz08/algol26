// Generate a JSON oracle: 10,000 (bit_pattern, expected_string)
// pairs from the running V8. Must be run with a recent Node.js
// (which uses the correct ECMA-262 §7.1.12.1 implementation).
const crypto = require('crypto');
const fs = require('fs');

const cases = [];
const seen = new Set();

function push(f) {
  const buf = Buffer.alloc(8);
  buf.writeDoubleBE(f);
  const hex = buf.toString('hex');
  if (seen.has(hex)) return false;
  seen.add(hex);
  cases.push({ hex, s: String(f) });
  return true;
}

const handPicked = [
  0, -0, 1, -1, 0.5, -0.5, 0.1, -0.1, 1.5, -1.5, 12.0, -12.0, 3.14,
  Number.MIN_VALUE, -Number.MIN_VALUE, Number.MAX_VALUE, -Number.MAX_VALUE,
  Number.EPSILON, 1e-7, -1e-7, 1e-6, -1e-6, 1e-5, 1e20, 1e21, -1e21, 1e22,
  NaN, Infinity, -Infinity,
  0.000001, 0.0000001, 100000000000000000000, 1000000000000000000000,
];
for (const f of handPicked) push(f);

// Random f64 bit patterns, skipping duplicates.
const buf = Buffer.alloc(8);
while (cases.length < 10000) {
  crypto.randomFillSync(buf);
  const f = buf.readDoubleBE();
  push(f);
}

fs.writeFileSync(
  require('path').join(__dirname, '..', 'tests', 'float_to_string', 'oracle.json'),
  JSON.stringify(cases, null, 0)
);
console.log(`wrote ${cases.length} cases`);
