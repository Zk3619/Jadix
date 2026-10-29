import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const directory = process.argv[2];
const backend = process.argv[3] || 'gl';
function readCsv(path) {
  const [header, ...lines] = readFileSync(path, 'utf8').trim().replace(/^\uFEFF/, '').split(/\r?\n/);
  const fields = header.split(',');
  return lines.map(line => Object.fromEntries(line.split(',').map((value, i) =>
    [fields[i], fields[i] === 'scenario' || fields[i] === 'pixel_hash' ? value : Number(value)])));
}
const percentile = (values, fraction) => [...values].sort((a, b) => a - b)[Math.floor((values.length - 1) * fraction)];
const mean = values => values.reduce((a, b) => a + b, 0) / values.length;
const data = {};
for (const variant of ['baseline', 'patched']) {
  const files = readdirSync(directory).filter(name => new RegExp(`^${backend}-${variant}-\\d+\\.csv$`).test(name));
  if (!files.length) throw new Error(`No timing runs for ${backend}/${variant}`);
  data[variant] = files.flatMap(file => readCsv(join(directory, file)).map(row => ({ ...row, run: file })));
  data[`${variant}_metrics`] = readCsv(join(directory, `${backend}-${variant}-metrics.csv`));
  if (!data[variant].length || !data[`${variant}_metrics`].length) throw new Error(`Empty benchmark: ${backend}/${variant}`);
}
const summary = [];
for (const scenario of new Set(data.baseline.map(row => row.scenario))) {
  const entry = { scenario };
  for (const variant of ['baseline', 'patched']) {
    const rows = data[variant].filter(row => row.scenario === scenario);
    const metrics = data[`${variant}_metrics`].filter(row => row.scenario === scenario);
    entry[variant] = {
      samples: rows.length,
      prepare_median_us: percentile(rows.map(row => row.prepare_us), .5),
      prepare_p95_us: percentile(rows.map(row => row.prepare_us), .95),
      complete_median_us: percentile(rows.map(row => row.complete_us), .5),
      complete_p95_us: percentile(rows.map(row => row.complete_us), .95),
      run_prepare_medians_us: [...new Set(rows.map(row => row.run))].map(run => percentile(rows.filter(row => row.run === run).map(row => row.prepare_us), .5)),
      uploads_mean: mean(metrics.map(row => row.uploads)),
      upload_bytes_mean: mean(metrics.map(row => row.upload_bytes)),
      mirror_bytes_max: Math.max(...metrics.map(row => row.mirror_bytes)),
      gpu_atlas_bytes_max: Math.max(...metrics.map(row => row.gpu_atlas_bytes)),
      raster_calls_mean: mean(metrics.map(row => row.raster_calls)),
      hash: metrics.at(-1).pixel_hash,
    };
  }
  entry.prepare_speedup = entry.baseline.prepare_median_us / entry.patched.prepare_median_us;
  entry.complete_speedup = entry.baseline.complete_median_us / entry.patched.complete_median_us;
  entry.pixels_equal = entry.baseline.hash === entry.patched.hash;
  const timingHashes = variant => data[variant].filter(row => row.scenario === scenario && row.pixel_hash !== '0').map(row => row.pixel_hash);
  entry.pixels_equal &&= JSON.stringify(timingHashes('baseline')) === JSON.stringify(timingHashes('patched'));
  if (!entry.pixels_equal) throw new Error(`Output pixels differ: ${scenario}`);
  summary.push(entry);
}
writeFileSync(join(directory, `${backend}-summary.json`), JSON.stringify(summary, null, 2));
console.table(summary.map(row => ({
  scenario: row.scenario,
  'base prepare us': row.baseline.prepare_median_us,
  'patch prepare us': row.patched.prepare_median_us,
  'prepare x': row.prepare_speedup.toFixed(2),
  'base complete us': row.baseline.complete_median_us,
  'patch complete us': row.patched.complete_median_us,
  'base uploads': row.baseline.uploads_mean,
  'patch uploads': row.patched.uploads_mean,
  'base KiB': (row.baseline.upload_bytes_mean / 1024).toFixed(1),
  'patch KiB': (row.patched.upload_bytes_mean / 1024).toFixed(1),
  'mirror MiB': (row.patched.mirror_bytes_max / 1048576).toFixed(2),
  pixels: row.pixels_equal,
})));
