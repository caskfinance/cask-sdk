// Run from crates/cask-node: npm run build && node ../../benches/node_bench.js
const { spawn, execFileSync } = require('node:child_process')
const net = require('node:net')
const path = require('node:path')

const { Cask } = require(path.resolve(__dirname, '../crates/cask-node'))

const ROOT = path.resolve(__dirname, '..')
const SIZES = [[1000, 20, 10], [100000, 500, 50]]
const TOTAL = 1_000_000
const SAMPLED = 200_000

const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

function freePort() {
  return new Promise((resolve) => {
    const srv = net.createServer().listen(0, '127.0.0.1', () => {
      const { port } = srv.address()
      srv.close(() => resolve(port))
    })
  })
}

async function bench([customers, pvs, features]) {
  console.log(`\n== ${customers} customers, ${pvs} product versions, ${features} features ==`)
  execFileSync('cargo', ['build', '-q', '--release', '-p', 'cask-mock-server'], { cwd: ROOT })
  const port = await freePort()
  const proc = spawn(
    path.join(ROOT, 'target/release/cask-mock-server'),
    ['--addr', `127.0.0.1:${port}`, '--synthetic', `${customers},${pvs},${features}`],
    { stdio: 'ignore' },
  )
  try {
    await sleep(customers > 10000 ? 1500 : 500)
    const t0 = process.hrtime.bigint()
    const cask = new Cask('test-key', { baseUrl: `http://127.0.0.1:${port}` })
    await cask.waitUntilReady(60)
    console.log(`download+parse+index: ${(Number(process.hrtime.bigint() - t0) / 1e6).toFixed(1)} ms`)

    const ids = Array.from({ length: customers }, (_, i) => `customer_${i.toString(16).toUpperCase().padStart(32, '0')}`)
    const names = Array.from({ length: features }, (_, i) => `feature_${i}`)
    const fns = [
      (c, f) => cask.checkBool(c, f),
      (c, f) => cask.checkNumeric(c, f),
      (c, f) => cask.checkEnum(c, f),
    ]
    const run = (i) => {
      const f = i % features
      return fns[f % 3](ids[(i * 2654435761) % customers], names[f])
    }

    let t = process.hrtime.bigint()
    for (let i = 0; i < TOTAL; i++) run(i)
    const mean = Number(process.hrtime.bigint() - t) / TOTAL

    const samples = new Array(SAMPLED)
    for (let i = 0; i < SAMPLED; i++) {
      t = process.hrtime.bigint()
      run(i)
      samples[i] = Number(process.hrtime.bigint() - t)
    }
    samples.sort((a, b) => a - b)
    const pct = (p) => samples[Math.floor((samples.length - 1) * p)]
    console.log(
      `check: mean ${mean.toFixed(0)} ns | p50 ${pct(0.5)} ns | p99 ${pct(0.99)} ns | p99.9 ${pct(0.999)} ns | max ${samples[samples.length - 1]} ns`,
    )
    cask.close()
  } finally {
    proc.kill()
  }
}

;(async () => {
  for (const size of SIZES) await bench(size)
})()
