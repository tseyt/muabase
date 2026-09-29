# Quickstart: Build a P2P Rental Marketplace on MuamalatOS (15 Minutes)

This quickstart guides you through building a peer-to-peer luxury fashion/equipment rental marketplace (competing directly with platforms like Pickle) using `@muamalatos/headless-sdk`. 

Key architectural advantages:
- **93% Lender Take-Home:** Slashes incumbent platform take-rates (typically 20–30%) down to 4% protocol fee + 3% peer mutual protection (*Takaful*).
- **Zero-*Riba* Deterrence:** 100% of punitive late-return penalties are hard-routed to a designated community charity sink (`charity_sink_account`).
- **Peer Triage Committee (*Halaqa*):** Disputed returns are arbitrated by 3 independent guild peers via photo evidence hashes rather than opaque centralized customer support.
- **Dual-Locale Branding:** Seamlessly toggle between standard institutional UI labels and explicit *Fiqh al-Mu‘amalat* terms.

---

## 1. Installation

Install the headless SDK and its peer dependencies in your Next.js, Vite, or React Native project:

```bash
npm install @muamalatos/headless-sdk
```

---

## 2. Initialize the Client

Configure the client to connect to your local `psinode` or testnet RPC gateway:

```typescript
// lib/muamalat.ts
import { MuamalatClient } from '@muamalatos/headless-sdk';

export const client = new MuamalatClient({
  endpoint: process.env.NEXT_PUBLIC_PSIBASE_RPC_URL || 'http://localhost:8080',
  defaultLocale: 'institutional', // or 'muamalat'
  appName: 'PickleKiller-FashionGuild',
});
```

---

## 3. Real-Time Guild Catalog Hook

Use `useMuamalatTable` to subscribe to active rental inventory. Data updates reactively via Server-Sent Events (SSE) on each block commit:

```tsx
// components/ItemCatalog.tsx
'use client';

import React from 'react';
import { useMuamalatTable } from '@muamalatos/headless-sdk';
import { client } from '@/lib/muamalat';

export function ItemCatalog() {
  const { data: leases, loading, error } = useMuamalatTable(
    client,
    'lease-engine',
    'UsufructLeaseTable',
    { status: 'Active' },
    5000 // Poll fallback interval (ms) if SSE reconnects
  );

  if (loading) return <div>Loading verified inventory...</div>;
  if (error) return <div>Error loading catalog: {error.message}</div>;

  return (
    <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
      {leases.map((lease: any) => (
        <div key={lease.lease_id} className="border rounded p-4 shadow-sm">
          <h3 className="font-bold">Asset #{lease.asset_id}</h3>
          <p className="text-sm text-gray-600">Lessor: {lease.lessor}</p>
          <p className="text-sm">Rate: ${(lease.rental_rate_per_day / 100).toFixed(2)} / day</p>
          <p className="text-xs text-green-700">
            Lender Take-Home: 93% (${((lease.rental_rate_per_day * 0.93) / 100).toFixed(2)})
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
import { client } from '@/lib/muamalat';

export async function rentLuxuryItem(params: {
  assetId: number;
  lessor: string;
  lessee: string;
  days: number;
  dailyRateCents: number;
  depositCents: number;
  charitySinkAccount: string;
}) {
  const now = Math.floor(Date.now() / 1000);
  const leaseEnd = now + params.days * 86400;

  // Execute createLease transaction via Psibase Wasm RPC
  const result = await client.createLease({
    asset_id: params.assetId,
    lessor: params.lessor,
    lessee: params.lessee,
    rental_rate_per_day: params.dailyRateCents,
    deposit_amount: params.depositCents,
    lease_start: now,
    lease_end: leaseEnd,
    charity_sink_account: params.charitySinkAccount,
  });

  console.log(`Lease created! TxHash: ${result.transaction_id}`);
  return result;
}
```

---

## 5. Non-Custodial Multi-Chain / Stripe Split Payment

If settling outside the native chain (e.g., Stripe Connect, Bitcoin Taproot P2TR, or EVM Safe), dispatch an `ExternalVault` intent:

```typescript
// actions/splitSettlement.ts
import { client } from '@/lib/muamalat';

export async function initiateStripeSplit(leaseId: number, grossAmountCents: number) {
  // Generates split intent: 93% Lessor, 4% Platform, 3% Mutual Shield
  const intent = await client.requestExternalSplit({
    vault_type: 'StripeConnect',
    source_reference: `lease_${leaseId}`,
    total_amount: grossAmountCents,
    currency: 'USD',
    splits: [
      { recipient: 'acct_lessor_123', share_bps: 9300 }, // 93%
      { recipient: 'acct_platform_dao', share_bps: 400 }, // 4%
      { recipient: 'acct_takaful_pool', share_bps: 300 }, // 3%
    ],
  });

  return intent;
}
```

---

## 6. Returning an Item: Clean Return vs. Photo Damage Claim

When the rental period concludes, the borrower or lender submits return status:

### Clean Return
```typescript
import { client } from '@/lib/muamalat';

export async function markReturnedClean(leaseId: number) {
  // Releases 100% of security deposit back to lessee
  return await client.returnClean(leaseId);
}
```

### Disputed Return (Triggering Peer Triage Committee)
```typescript
import { client } from '@/lib/muamalat';

export async function fileDamageClaim(leaseId: number, photoHashHex: string, claimNotes: string) {
  // Enters STATUS_DISPUTED and initiates ClaimCouncil 3-peer evaluation
  return await client.disputeReturn({
    lease_id: leaseId,
    evidence_hash: photoHashHex,
    notes: claimNotes,
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
import { client } from '@/lib/muamalat';

export function LocaleToggle() {
  const [locale, setLocale] = useState<'institutional' | 'muamalat'>('institutional');

  const toggle = () => {
    const next = locale === 'institutional' ? 'muamalat' : 'institutional';
    setLocale(next);
    client.setLocale(next);
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
        {client.t('lease')} | {client.t('shield')} | {client.t('rofr')}
      </span>
    </div>
  );
}
```

---

## 8. Summary of Mathematical Guarantees

| Invariant | Implementation Mechanism | Regulatory / Jurisprudential Reference |
|---|---|---|
| **Zero-*Riba*** | Punitive late fees routed to `charity_sink_account` | Qur'an 2:278–279; AAOIFI Shariah Std 8 |
| **93% Payout** | `9300 bps` / `400 bps` / `300 bps` fixed split | Disintermediation of 25–30% Web2 fee models |
| **Non-Custodial** | 0-key `ExternalVault` client-side multisig / Stripe split | NYC BitLicense 23 NYCRR § 200.2(q) |
| **Peer Triage** | 3-witness Condorcet rank-order in `ClaimCouncil` | AAOIFI Shariah Std 5 (Guarantees & Indemnities) |
