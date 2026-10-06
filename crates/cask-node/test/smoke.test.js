const { test, before, after } = require('node:test')
const assert = require('node:assert/strict')
const { spawn, execFileSync } = require('node:child_process')
const net = require('node:net')
const path = require('node:path')

const { Cask } = require('..')

const ROOT = path.resolve(__dirname, '../../..')
const FREE = 'customer_01A10971485672EFB40302EAE6021F01'
const PRO = 'customer_01A10971485672EFB40302EAE6021F02'
const PRO_UNLIMITED = 'customer_01A10971485672EFB40302EAE6021F03'

let proc
let baseUrl

function freePort() {
  return new Promise((resolve) => {
    const srv = net.createServer().listen(0, '127.0.0.1', () => {
      const { port } = srv.address()
      srv.close(() => resolve(port))
    })
  })
}

function canConnect(port) {
  return new Promise((resolve) => {
    const s = net.connect(port, '127.0.0.1')
    s.on('connect', () => (s.destroy(), resolve(true)))
    s.on('error', () => resolve(false))
  })
}

before(async () => {
  execFileSync('cargo', ['build', '-q', '-p', 'cask-mock-server'], { cwd: ROOT })
  const port = await freePort()
  proc = spawn(path.join(ROOT, 'target/debug/cask-mock-server'), ['--addr', `127.0.0.1:${port}`], {
    stdio: 'ignore',
  })
  for (let i = 0; i < 50 && !(await canConnect(port)); i++) {
    await new Promise((r) => setTimeout(r, 100))
  }
  baseUrl = `http://127.0.0.1:${port}`
})

after(() => proc.kill())

test('checks', async () => {
  const cask = new Cask('test-key', { baseUrl })
  try {
    await cask.waitUntilReady(5)
    assert.equal(cask.checkBool(FREE, 'sso'), false)
    assert.equal(cask.checkBool(PRO, 'sso'), true)
    assert.equal(cask.checkNumeric(FREE, 'seats'), 3)
    assert.equal(cask.checkNumeric(PRO_UNLIMITED, 'seats'), Infinity)
    assert.equal(cask.checkEnum(PRO, 'support_tier'), 'priority')

    assert.throws(() => cask.checkBool('nope', 'sso'), { code: 'CustomerNotFound' })
    assert.throws(() => cask.checkBool(FREE, 'nope'), { code: 'FeatureNotFound' })
    assert.throws(() => cask.checkBool(FREE, 'seats'), { code: 'WrongType' })
  } finally {
    cask.close()
  }
})

test('NotReady before load', async () => {
  const cask = new Cask('test-key', { baseUrl: 'http://127.0.0.1:1' })
  try {
    assert.throws(() => cask.checkBool(FREE, 'sso'), { code: 'NotReady' })
    await assert.rejects(cask.waitUntilReady(0.2))
  } finally {
    cask.close()
  }
})

test('bad key', async () => {
  const cask = new Cask('wrong', { baseUrl })
  try {
    await assert.rejects(cask.waitUntilReady(2), /authentication failed/)
  } finally {
    cask.close()
  }
})
