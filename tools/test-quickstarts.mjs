import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';

const QUICKSTARTS = [
  'doc/HUMANITARIAN_CORRIDOR_QUICKSTART.md',
  'doc/WAQF_MASJID_TREASURY_QUICKSTART.md',
  'doc/MUAMALAT_TAHKIM_QUICKSTART.md',
];

const VALID_SERVICES = new Set([
  'credential',
  'staged-tx',
  'cred-clear',
  'ext-vault',
  'waqf-treas',
  'muamalat',
  'auth-delg',
  'producers',
  'tokens',
]);

const EXAMPLE_QUICKSTARTS = [
  'packages/shared-ui/headless-sdk/examples/fashion-guild-quickstart.md',
  'packages/shared-ui/headless-sdk/examples/beli-competitor-quickstart.md',
  'packages/shared-ui/headless-sdk/examples/mizanvest-quickstart.md',
  'packages/shared-ui/headless-sdk/examples/irfan-markets-quickstart.md',
];

const HASH_FIELD_RE = /(?:^|_)hash$/;

test('GIVEN all three institutional quickstarts WHEN audited THEN zero fictional CLI flags or unresolvable IP subdomains exist, all JSON actions parse, and all 32-byte hashes are 64 hex chars', () => {
  for (const path of QUICKSTARTS) {
    assert.ok(existsSync(path), `Missing quickstart: ${path}`);
    const md = readFileSync(path, 'utf8');

    // Guard against known fictional CLI drift and unresolvable IP subdomains
    assert.ok(!md.includes('psinode --database'), `${path} must not use fictional --database flag`);
    assert.ok(!md.includes('psibase create-account'), `${path} must not use fictional create-account subcommand`);
    assert.ok(!md.includes('psibase boot --host'), `${path} must use psibase -a ... boot -p ...`);
    assert.ok(!md.includes('psibase push --host'), `${path} must use psibase -a ... push <file>`);
    assert.ok(!/https?:\/\/[a-z0-9-]+\.127\.0\.0\.1/.test(md), `${path} must not use unresolvable *.127.0.0.1 subdomain URLs`);

    // Parse every fenced ```json block and validate canonical service slugs & 32-byte hash lengths
    const jsonBlocks = [...md.matchAll(/```json\n([\s\S]*?)```/g)];
    assert.ok(jsonBlocks.length >= 2, `${path} should include runnable JSON action examples`);
    for (const [, raw] of jsonBlocks) {
      const parsed = JSON.parse(raw);
      assert.ok(Array.isArray(parsed), `${path} push JSON block must be an action array`);
      for (const action of parsed) {
        assert.ok(action.sender && action.service && action.method && action.data, 'Incomplete action object');
        assert.ok(VALID_SERVICES.has(action.service), `Non-canonical service '${action.service}' in ${path}`);
        for (const [key, val] of Object.entries(action.data)) {
          if (HASH_FIELD_RE.test(key) && typeof val === 'string' && !val.startsWith('<')) {
            const clean = val.replace(/^0x/i, '');
            assert.match(clean, /^[0-9a-fA-F]{64}$/, `${path} field '${key}' must be 64 hex chars, got length ${clean.length}`);
          }
        }
      }
    }
  }
});
test('GIVEN llms.txt, SDK examples, and Studio UI WHEN scanned for drifted service slugs THEN only canonical Wasm service slugs appear', () => {
  const STALE_SLUGS = [
    'claimcouncil',
    'creditclear',
    'tokenstream',
    'mutualshield',
    'psinode-saas',
    'auth-delegate',
  ];
  const FILES = [
    'llms.txt',
    'packages/shared-ui/headless-sdk/examples/irfan-markets-quickstart.md',
    'packages/shared-ui/headless-sdk/examples/mizanvest-quickstart.md',
    'packages/shared-ui/headless-sdk/examples/beli-competitor-quickstart.md',
    'packages/shared-ui/headless-sdk/examples/fashion-guild-quickstart.md',
    'packages/user/Studio/ui/src/lib/studio-data.ts',
    'packages/user/Studio/ui/src/context/studio-context.tsx',
    'packages/user/Studio/ui/src/pages/overview-page.tsx',
    'packages/user/Studio/ui/src/pages/actions-playground-page.tsx',
    'packages/user/Studio/ui/src/pages/invariants-studio-page.tsx',
  ];

  for (const file of FILES) {
    assert.ok(existsSync(file), `Missing audited file: ${file}`);
    const content = readFileSync(file, 'utf8');
    for (const stale of STALE_SLUGS) {
      assert.ok(
        !content.includes(stale),
        `${file} must not reference stale service slug '${stale}'`,
      );
    }
  }
});

test('GIVEN headless-sdk example quickstarts WHEN audited THEN no deprecated service slugs or syntax-breaking quotes exist', () => {
  for (const path of EXAMPLE_QUICKSTARTS) {
    assert.ok(existsSync(path), `Missing example quickstart: ${path}`);
    const md = readFileSync(path, 'utf8');
    assert.ok(!md.includes("table: 'Ju'alahBountyTable'"), `${path} contains unescaped quote syntax error`);
    assert.ok(!/service:\s*['"](?:evals|tokenstream|claimcouncil|creditclear)['"]/.test(md), `${path} uses non-canonical service slug`);
  }
});

test('GIVEN llms.txt and compliance.md WHEN audited THEN all documented Wasm service slugs are canonical', () => {
  const forbidden = ['"claimcouncil"', '"creditclear"', '"mutualshield"', '"tokenstream"', '"auth-delegate"', '"credentials"'];
  for (const path of ['llms.txt', 'compliance.md']) {
    const md = readFileSync(path, 'utf8');
    for (const bad of forbidden) {
      assert.ok(!md.includes(bad), `${path} must not contain non-canonical service slug ${bad}`);
    }
  }
});
