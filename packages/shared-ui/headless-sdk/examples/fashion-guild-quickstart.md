# Quickstart: Build a P2P Rental Marketplace on Muabase (15 Minutes)

This quickstart guides you through building a high-trust peer-to-peer luxury fashion and equipment rental marketplace (outcompeting Web2 platforms like Pickle) using `@muabase/headless-sdk`. 

Key architectural advantages:
- **93% Lender Take-Home:** Slashes incumbent platform take-rates (typically 20–30%) down to 4% protocol fee + 3% peer mutual protection pool (`mutual_shield` / *Takaful*).
- **Zero-*Riba* Deterrence:** 100% of punitive late-return penalties are hard-routed to a designated community charity sink (`charity_sink`, contract field `charity_sink_account`).
- **Peer Triage Committee (`claim_council` / *Majlis al-Tahkim*):** Disputed returns are arbitrated by 3 independent guild peers via photo evidence hashes rather than opaque centralized customer support.
- **Dual-Witness Evidentiary Gate (*Bayyinah*):** Before claims or releases execute, 2 qualified witnesses holding soulbound reliability (`trust_score` / `AdalahScore`) and competence (`domain_badge` / `IkhtisasTag`) attest to the evidence hash.
- **Co-Owner Priority Window (`priority_window` / *Haqq al-Shuf‘ah*):** Fractional co-owners enjoy an automated pre-emption window before external bookings or purchases execute.
- **Dual-Locale Branding:** Seamlessly toggle between standard institutional UI labels and explicit *Fiqh al-Mu‘amalat* terms.

---

## 1. Installation

Install the headless SDK and its peer dependencies in your Next.js, Vite, or React Native project:

```bash
npm install @muabase/headless-sdk
```

---

## 2. Initialize the Client

Configure the client to connect directly to your local `psinode` or testnet RPC gateway without backend middleware:

```typescript
// lib/muabase.ts
import { createMuabaseClient } from '@muabase/headless-sdk';

export const client = createMuabaseClient({
  endpoint: process.env.NEXT_PUBLIC_PSIBASE_RPC_URL || 'http://localhost:8080',
  locale: 'institutional', // or 'muamalat'
});
```

---

## 3. List a Luxury Asset for Rental (Lender Flow)

Lenders register physical garments, accessories, or camera kits into the guild's `asset_pool` (`AssetPoolConfig`). The item is tokenized with tangible asset floor guarantees and trust gating:

```typescript
// actions/listAsset.ts
import { client } from '@/lib/muabase';

export async function listDesignerGarment(params: {
  title: string;
  dayRateUsd: number;
  replacementValueUsd: number;
}) {
  return await client.guild.assets.create({
    title: params.title,
    dayRateUsd: params.dayRateUsd,
    replacementValueUsd: params.replacementValueUsd,
    minTrustScore: 7000,       // Minimum trust_score (70.00%) to rent or hold
    tangibleRatioBps: 10000,   // 100% tangible asset backing (tangible_ratio >= 5100)
    priorityWindowSec: 86400,  // 24h co-owner priority window (Haqq al-Shuf'ah)
  });
}
```

---

## 4. Real-Time Guild Catalog Hook

Use `useMuabaseTable` to subscribe to active rental inventory (`lease`). Data updates reactively via Server-Sent Events (SSE) directly from memory-mapped Trie storage (`Triedent`):

```tsx
// components/ItemCatalog.tsx
'use client';

import React from 'react';
import { useMuabaseTable } from '@muabase/headless-sdk';
import { client } from '@/lib/muabase';

export function ItemCatalog() {
  const { data: leases, loading, error } = useMuabaseTable({
    client,
    service: 'lease-eng',
    table: 'UsufructLeaseTable',
    query: '{ usufructLeaseTable { leaseId lessor lessee grossPayment status } }',
    filter: (lease: any) => lease.status === 'Active',
    pollIntervalMs: 5000, // Fallback interval (ms) if SSE reconnects
  });

  if (loading) return <div>Loading verified inventory...</div>;
  if (error) return <div>Error loading catalog: {error.message}</div>;

  return (
    <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
      {leases.map((lease: any) => (
        <div key={lease.leaseId} className="border rounded p-4 shadow-sm">
          <h3 className="font-bold">Asset #{lease.assetId || lease.leaseId}</h3>
          <p className="text-sm text-gray-600">Lessor: {lease.lessor}</p>
          <p className="text-sm">Rate: ${(lease.grossPayment / 10000).toFixed(2)}</p>
          <p className="text-xs text-green-700">
            Lender Take-Home: 93% (${((lease.grossPayment * 0.93) / 10000).toFixed(2)})
          </p>
        </div>
      ))}
    </div>
  );
}
```

---

## 5. One-Click Checkout with 93%/4%/3% Split

Borrowers book an active `lease` with client-side photographic condition attestation:

```typescript
// actions/checkout.ts
import { client } from '@/lib/muabase';

export async function rentLuxuryItem(params: {
  assetId: number;
  durationDays: number;
  preConditionEvidenceHash: string;
}) {
  const lease = await client.guild.leases.checkout({
    assetId: params.assetId,
    durationDays: params.durationDays,
    preConditionEvidenceHash: params.preConditionEvidenceHash,
    rail: 'stripe_connect',
  });

  console.log(`Lease created! Lease ID: ${lease.leaseId}`);
  return lease;
}
```

---

## 6. Non-Custodial Multi-Chain / Stripe Split Payment

When settling outside the native chain (e.g., Stripe Connect destination charges, Bitcoin Taproot P2TR, or EVM Safe), dispatch an `ExternalVault` intent. The protocol maintainer holds **zero keys**:

```typescript
// actions/splitSettlement.ts
import { client } from '@/lib/muabase';

export async function initiateStripeSplit(leaseId: number, grossAmountCents: number) {
  // Generates split intent: 93% Lender, 4% Mutual Shield, 3% SaaS Operator
  const intent = await client.transact('ext-vault', 'generateSplitIntent', {
    leaseId,
    railType: 'stripe_connect',
    grossAmountCents,
    lenderAccount: 'asset-owner',
  });

  return intent;
}
```

---

## 7. Returning an Item: Clean Return vs. Photo Damage Claim

When the rental period concludes, the condition is confirmed:

### Clean Return
```typescript
import { client } from '@/lib/muabase';

export async function markReturnedClean(leaseId: string, postConditionHash: string) {
  // Releases 100% of security deposit back to borrower and increments lender/borrower trust_score
  return await client.guild.leases.confirmReturn({
    leaseId,
    postConditionEvidenceHash: postConditionHash,
  });
}
```

### Disputed Return (Triggering Peer Triage Committee)
```typescript
import { client } from '@/lib/muabase';

export async function fileDamageClaim(params: {
  leaseId: string;
  claimedAmountUsd: number;
  preConditionHash: string;
  postConditionHash: string;
  repairQuoteHash: string;
}) {
  // Enters Disputed status and dispatches to ClaimCouncil 3-peer evaluation (Bayyinah)
  return await client.guild.claims.submitEvidence({
    leaseId: params.leaseId,
    claimedAmountUsd: params.claimedAmountUsd,
    preConditionEvidenceHash: params.preConditionHash,
    postConditionEvidenceHash: params.postConditionHash,
    repairQuoteHash: params.repairQuoteHash,
  });
}
```

---

## 8. Dual-Locale Terminology Switcher

Support both standard institutional terminology and authentic *Fiqh al-Mu‘amalat* terms:

```tsx
// components/LocaleToggle.tsx
'use client';

import React, { useState } from 'react';
import { client } from '@/lib/muabase';

export function LocaleToggle() {
  const [locale, setLocale] = useState<'institutional' | 'muamalat'>('institutional');

  const toggle = () => {
    const next = locale === 'institutional' ? 'muamalat' : 'institutional';
    setLocale(next);
    client.locale = next;
  };

  return (
    <div className="flex items-center space-x-3">
      <button 
        onClick={toggle}
        className="px-3 py-1 text-sm bg-gray-800 text-white rounded hover:bg-gray-700"
      >
        Mode: {locale === 'institutional' ? '🏢 Commercial Institutional' : '⚖️ Fiqh al-Mu‘amalat'}
      </button>

      <span className="text-xs text-gray-500">
        Current Mode: {locale}
      </span>
    </div>
  );
}
```

---

## 9. Summary of Mathematical Guarantees

| Invariant | Canonical Key | Contract / Wasm Mechanism | Regulatory / Jurisprudential Reference |
| :--- | :--- | :--- | :--- |
| **Zero-*Riba*** | `charity_sink` | 100% of late fees routed to `charity_sink_account` | Qur'an 2:278–279; AAOIFI Shariah Std 8 (Ijarah) |
| **93% Payout** | `lease` | `9300 bps` / `400 bps` / `300 bps` fixed split | Disintermediation of Web2 rent-extraction |
| **Non-Custodial** | `ExternalVault` | 0-key client-side multisig (Taproot / Safe / Stripe) | NYC BitLicense `23 NYCRR § 200.2(q)` Safe Harbor |
| **Binary *Bayyinah*** | `claim_council` / `evals` | 2-witness quorum (`trust_score` + `domain_badge`) | Qur'an 2:282; AAOIFI Shariah Std 5 (Guarantees) |
| **Tangible Asset Floor** | `tangible_ratio` | Non-par trading blocked if `tangible_ratio_bps < 5100` | AAOIFI Shariah Std 21 (Sukuk); Bay‘ al-Dayn |
| **Co-Owner ROFR** | `priority_window` | Automated pre-emption window (`priority_window_sec`) | Classical *Haqq al-Shuf‘ah* Pre-emption |
| **Soulbound Identity** | `trust_score` / `domain_badge` | Non-transferable `TrustScoreRecord` & `DomainBadgeRecord` | Pure software exemption; non-monetizable |
