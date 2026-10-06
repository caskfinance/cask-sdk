// Start the mock server first:
//   cargo run -p cask-mock-server -- --addr 127.0.0.1:8099
// then:
//   CASK_BASE_URL=http://127.0.0.1:8099 node examples/basic.js
const { Client } = require('..')

const CUSTOMER = 'customer_01A10971485672EFB40302EAE6021F03'

async function main() {
  const cask = new Client({ apiKey: 'test-key', baseUrl: process.env.CASK_BASE_URL })
  try {
    await cask.waitUntilReady(5)

    console.log('sso:', cask.checkBool(CUSTOMER, 'sso'))
    console.log('seats:', cask.checkNumeric(CUSTOMER, 'seats'))
    console.log('supportTier:', cask.checkEnum(CUSTOMER, 'support_tier'))

    try {
      cask.checkBool('nope', 'sso')
    } catch (e) {
      console.log('unknown customer:', e.code, e.message)
    }
  } finally {
    cask.close()
  }
}

main()
