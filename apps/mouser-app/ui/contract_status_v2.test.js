// Consumer side of the control/status v2 seam: every status fixture the UI
// tests use validates against the schema frozen in mouser-proto.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import Ajv from 'ajv/dist/2020.js';
import { STATUS_FIXTURES } from './status_fixture.js';

const schema = JSON.parse(
  readFileSync(new URL('../../../crates/mouser-proto/schemas/status.schema.json', import.meta.url), 'utf8'),
);
const validate = new Ajv({ strict: false, allErrors: true }).compile(schema);

test('there are fixtures for every kind of desk the UI tests describe', () => {
  assert.deepEqual(Object.keys(STATUS_FIXTURES).sort(), [
    'alone',
    'elevatedApp',
    'incompatiblePeer',
    'oldPeer',
    'twoMacsWindowsLinux',
  ]);
});

for (const [name, status] of Object.entries(STATUS_FIXTURES)) {
  test(`the ${name} fixture validates against status.schema.json`, () => {
    assert.equal(validate(status), true, JSON.stringify(validate.errors));
  });
}
