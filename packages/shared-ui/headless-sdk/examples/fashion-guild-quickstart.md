# Quickstart: Build a P2P Rental Marketplace on Muabase (15 Minutes)

This quickstart guides you through building a peer-to-peer luxury fashion/equipment rental marketplace (competing directly with platforms like Pickle) using `@muabase/headless-sdk`. 

Key architectural advantages:
- **93% Lender Take-Home:** Slashes incumbent platform take-rates (typically 20–30%) down to 4% protocol fee + 3% peer mutual protection (*Takaful*).
- **Zero-*Riba* Deterrence:** 100% of punitive late-return penalties are hard-routed to a designated community charity sink (`charity_sink` / `charity_sink_account`).
- **Peer Triage Committee (*Halaqa*):** Disputed returns are arbitrated by 3 independent guild peers via photo evidence hashes rather than opaque centralized customer support.
- **Dual-Locale Branding:** Seamlessly toggle between standard institutional UI labels and explicit *Fiqh al-Mu‘amalat* terms.

---

## 1. Installation

Install the headless SDK and its peer dependencies in your Next.js, Vite, or React Native project:

```bash
npm install @muabase/headless-sdk
```

---

## 2. Initialize the Client

Configure the client to connect to your local `psinode` or testnet RPC gateway:

```typescript
// lib/muabase.ts
import { MuabaseClient } from '@muabase/headless-sdk';

export const client = new MuabaseClient({
  endpoint: process.env.NEXT_PUBLIC_PSIBASE_RPC_URL || 'http://localhost:8080',
  defaultLocale: 'institutional', // or 'muamalat'
  appName: 'PickleKiller-FashionGuild',
});
```

---

## 3. Real-Time Guild Catalog Hook

Use `useMuabaseTable` to subscribe to active rental inventory. Data updates reactively via Server-Sent Events (SSE) on each block commit:

```tsx
// components/ItemCatalog.tsx
'use client';

import React from 'react';
import { useMuabaseTable } from '@muabase/headless-sdk';
import { client } from '@/lib/muabase';

export function ItemCatalog() {
  const { data: leases, loading, error } = useMuabaseTable({
    client,
    service: 'lease-engine',
    table: 'UsufructLeaseTable',
    query: '{ usufructLeaseTable { leaseId lessor lessee grossPayment status } }',
    filter: (lease: any) => lease.status === 'Active',
    pollIntervalMs: 5000, // Poll fallback interval (ms) if SSE reconnects
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

## 4. One-Click Checkout with 93%/4%/3% Split

Create an active lease agreement with client-side cryptographic attestation:

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

## 5. Non-Custodial Multi-Chain / Stripe Split Payment

If settling outside the native chain (e.g., Stripe Connect, Bitcoin Taproot P2TR, or EVM Safe), dispatch an `ExternalVault` intent:

```typescript
// actions/splitSettlement.ts
import { client } from '@/lib/muabase';

export async function initiateStripeSplit(leaseId: number, grossAmountCents: number) {
  // Generates split intent: 93% Lessor, 4% Platform, 3% Mutual Shield
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

## 6. Returning an Item: Clean Return vs. Photo Damage Claim

When the rental period concludes, the borrower or lender submits return status:

### Clean Return
```typescript
import { client } from '@/lib/muabase';

export async function markReturnedClean(leaseId: string, postConditionHash: string) {
  // Releases 100% of security deposit back to lessee
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
  // Enters STATUS_DISPUTED and initiates ClaimCouncil 3-peer evaluation
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

## 7. Dual-Locale Terminology Switcher

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

## 8. Summary of Mathematical Guarantees

| Invariant | Implementation Mechanism | Regulatory / Jurisprudential Reference |
|---|---|---|
| **Zero-*Riba*** | Punitive late fees routed to `charity_sink` (`charity_sink_account`) | Qur'an 2:278–279; AAOIFI Shariah Std 8 |
| **93% Payout** | `9300 bps` / `400 bps` / `300 bps` fixed split | Disintermediation of 25–30% Web2 fee models |
| **Non-Custodial** | 0-key `ExternalVault` client-side multisig / Stripe split | NYC BitLicense 23 NYCRR § 200.2(q) |
| **Peer Triage** | 3-witness Condorcet rank-order in `ClaimCouncil` | AAOIFI Shariah Std 5 (Guarantees & Indemnities) |
