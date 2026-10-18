# Quickstart: Build an Un-Gameable Social Restaurant Ranking & Tastemaker App on Muabase (15 Minutes)

This quickstart guides you through building a high-trust, social restaurant ranking and tastemaker platform—engineered specifically to **outcompete Web2 incumbents like Beli**—using `@muabase/headless-sdk`.

Beli demonstrated that modern diners crave pairwise restaurant comparisons, personal taste maps, and curated friend recommendations over outdated 5-star review platforms like Yelp or Google Maps. However, Beli suffers from the classic structural flaws of centralized Web2 platforms:
1. **Zero Creator Monetization:** Foodies, critics, and tastemakers spend thousands of dollars dining out and hundreds of hours curating lists, yet capture **0%** of the platform's financial upside.
2. **Review Astroturfing & Sybil Manipulation:** Unverified, self-reported dining claims allow PR agencies, burner accounts, and bot rings to game rankings with zero proof-of-visit.
3. **Black-Box Proprietary Algorithms:** Diners are subjected to opaque recommendation rankings vulnerable to future pay-to-play sponsored placements.
4. **Heavy Backend Infrastructure:** Building a Beli clone in Web2 requires orchestrating PostgreSQL, Redis caches, GraphQL microservices, and sync servers—costing thousands of dollars a month and months of dev time.
5. **No Native Transaction or Escrow Primitives:** Incumbents cannot facilitate tasting-menu reservations, private supper club ticketing, or group dining deposits without massive custodial liability.

**Muabase solves this entirely.** Running on the Psibase Wasm-in-Database architecture (`Triedent`), Muabase enables you to ship a hyper-fast, reactive dining app with cryptographic proof-of-visit, 93% creator revenue splits, and zero backend middleware.

---

## The Strategic Battle: Beli (Web2) vs. Your App on Muabase

| Feature / Dimension | Beli (Web2 Incumbent) | Your Challenger App on Muabase |
| :--- | :--- | :--- |
| **Creator Monetization** | **0%:** Curators and foodies work as unpaid data generators for the platform. | **93% Curator Take-Home:** Curators monetize private dining guides and supper clubs with transparent 93% splits (4% mutual shield, 3% SaaS operator). |
| **Visit Verification** | **Unverified:** Anyone can log visits to restaurants they never set foot in. | **Cryptographic *Bayyinah*:** Receipt SHA-256 hashes, POS check digests, and optional dual-witness attestations. |
| **Identity & Reputation** | Centralized follower counts & gamified badges. Highly vulnerable to Sybils. | **Soulbound Credentials:** Non-transferable reliability (`trust_score`) and domain expertise badges (`domain_badge`, e.g., "Verified Sommelier", "Michelin Inspector"). |
| **Pairwise Ranking** | Proprietary, opaque Elo-style algorithm running in a black-box database. | **Wasm-Native Condorcet Tournaments (`evals`):** Mathematically optimal, un-gameable pairwise ranking executed on-chain. |
| **Supper Clubs & Escrow** | None. Users must coordinate group dining and payments via Venmo/Splitwise. | **Non-Custodial Multi-Party Splits (`ExternalVault`):** Seamless Stripe Connect, EVM Safe, or Bitcoin Taproot splits. Operator holds **0 keys**. |
| **Cancellation Ethics** | Opaque platform cancellation fees pocketed entirely by third parties. | **Zero-*Riba* Food Rescue Sink (`charity_sink`):** Kitchens receive direct ingredient reimbursement; punitive late fees route 100% to a community food charity. |
| **Database & API Tier** | Multi-tier AWS stack (Postgres + Redis + Node/Go + WebSockets), 100–500ms latency. | **Serverless Wasm-in-Database (`Triedent`):** Microsecond read/write speed, direct client GraphQL, and reactive SSE streams. |
| **Regulatory Risk** | Custodial payment processing risks under state money transmission laws. | **NYC BitLicense Safe Harbor (`23 NYCRR § 200.2(q)`):** Pure software exemption; non-custodial cryptographic execution. |

---

## 1. Installation

Install `@muabase/headless-sdk` and its peer dependencies in your Next.js, Vite, or React Native project:

```bash
npm install @muabase/headless-sdk
```

---

## 2. Initialize the Muabase Client

Connect your frontend directly to your local `psinode` or hosted cluster. No Express, Fastify, or intermediate backend API is required:

```typescript
// lib/muabase.ts
import { createMuabaseClient } from '@muabase/headless-sdk';

export const client = createMuabaseClient({
  endpoint: process.env.NEXT_PUBLIC_PSIBASE_RPC_URL || 'http://localhost:8080',
  locale: 'institutional', // Toggle seamlessly between 'institutional' and 'muamalat'
  account: 'alice-foodie',
});
```

---

## Architectural Note: Mechanism vs. Policy

> **You control the client stack; Muabase guarantees the invariants.**  
> Notice that Muabase does not enforce a rigid, vertical-specific backend schema. Instead of bundling bespoke server logic, your client application maps its dining concepts directly onto Muabase's universal canonical primitives:
> - **Restaurant Profiles:** Mapped to canonical `asset_pool` via `client.tokens.configureAssetPool`.
> - **Proof-of-Visit Verification:** Mapped to canonical `evidence_gate` via `client.stagedTx.recordAttestation`.
> - **Pairwise Tastemaker Rankings:** Mapped to canonical `evals` via `client.evaluations.recordPairwiseVote`.
> - **Supper Club Escrow & 93% Splits:** Mapped to canonical `lease` via `client.leaseEngine.createLease` and `client.externalVault.generateSplitIntent`.
> - **Zero-Riba Cancellation Guard:** Mapped to canonical `charity_sink` via `client.leaseEngine.settleCancellation`.

---

## 3. Register Verified Restaurants into the Registry Pool

Restaurants are registered into the on-chain asset registry (`AssetPoolConfig`) with 100% tangible physical backing (`tangible_ratio_bps: 10000`) and minimum trust score gates to prevent spam:

```typescript
// actions/registerRestaurant.ts
import { client } from '@/lib/muabase';

export async function registerRestaurant(params: {
  name: string;
  cuisine: string;
  neighborhood: string;
  city: string;
  priceTier: 1 | 2 | 3 | 4;
}) {
  // Call Muabase canonical tokens service directly
  return await client.tokens.configureAssetPool({
    tokenId: 3001,
    legalWrapperHash: "0x" + Buffer.from(params.name).toString("hex").padEnd(64, "0"),
    maxHolders: 10000,
    minTrustScore: 6500,     // Minimum 65.00% reliability score to submit official ratings
    tangibleRatioBps: 10000, // 100% physical tangible dining experience (tangible_ratio >= 5100)
    priorityWindowSec: 86400,// 24h Right of First Refusal window
    custodianAccount: "restaurant-registry",
    custodianFeeBps: 0,
    charitySinkAccount: "waqf-sadaqah",
  });
}
```

---

## 4. Cryptographic Proof-of-Visit (*Bayyinah* Receipt Attestation)

This is your primary weapon against Beli's fake reviews. When a user logs a visit, your app hashes their itemized POS receipt or photo evidence (`receiptDigestHash`). The transaction verifies proof-of-visit and rewards the diner with soulbound reliability points (`trust_score`):

```typescript
// actions/logVisit.ts
import { client } from '@/lib/muabase';

export async function logVerifiedDiningExperience(params: {
  restaurantId: number;
  receiptImageBuffer: ArrayBuffer;
  overallRatingBps: number; // e.g., 9250 = 9.25 / 10.0
  dishesOrdered: string[];
  notes?: string;
  coDinerWitness?: string; // Optional dual-witness Bayyinah co-signature
}) {
  // Compute SHA-256 digest of receipt / bill
  const hashBuffer = await crypto.subtle.digest('SHA-256', params.receiptImageBuffer);
  const receiptDigestHash = '0x' + Array.from(new Uint8Array(hashBuffer))
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');

  // Submit to canonical stagedTx service (Bayyinah Dual-Witness Gate)
  return await client.stagedTx.recordAttestation({
    service: "evaluation",
    entityId: params.restaurantId,
    evidenceHash: receiptDigestHash,
    attester: client.account,
    witnessQuorum: params.coDinerWitness ? 2 : 1,
    metadata: {
      overallRatingBps: params.overallRatingBps,
      dishes: params.dishesOrdered,
      notes: params.notes,
      timestamp: Math.floor(Date.now() / 1000),
    },
  });
}
```

---

## 5. Pairwise Condorcet Comparison Engine (Outperforming Beli's Elo)

When a diner logs a spot, present them with head-to-head comparisons against places they’ve previously visited. Instead of an opaque black-box Elo, Muabase resolves rankings via **Schulze Condorcet tournaments** directly inside the `Evaluations` (`evals`) Wasm microservice:

```typescript
// actions/rankComparison.ts
import { client } from '@/lib/muabase';

export async function submitHeadToHeadChoice(params: {
  winnerRestaurantId: number;
  loserRestaurantId: number;
  category?: 'overall' | 'food' | 'vibe' | 'service';
  receiptDigestHash?: string;
}) {
  // Submits pairwise ballot to canonical Wasm evaluations service (evals / Ju'alah Halaqa)
  return await client.evaluations.recordPairwiseVote({
    winnerId: params.winnerRestaurantId,
    loserId: params.loserRestaurantId,
    category: params.category || 'overall',
    voter: client.account,
    receiptHash: params.receiptDigestHash || null,
  });
}
```

---

## 6. Real-Time Reactive Tastemaker Feed Hook

Subscribe your mobile UI or React web app directly to live restaurant rankings and recent verified logs using `useMuabaseTable`. State changes stream via Server-Sent Events (SSE) directly from memory-mapped Trie storage (`Triedent`) with microsecond latency:

```tsx
// components/TastemakerFeed.tsx
'use client';

import React from 'react';
import { useMuabaseTable } from '@muabase/headless-sdk';
import { client } from '@/lib/muabase';

export function TastemakerFeed() {
  const { data: logs, loading, error } = useMuabaseTable({
    client,
    service: 'evaluation',
    table: 'AttestationRecordTable',
    query: '{ attestationRecordTable { entityId attester metadata timestamp } }',
    pollIntervalMs: 3000,
  });

  if (loading) return <div className="p-4 text-gray-500">Connecting to Wasm Trie Feed...</div>;
  if (error) return <div className="p-4 text-red-500">Error: {error.message}</div>;

  return (
    <div className="max-w-xl mx-auto space-y-4">
      <h2 className="text-2xl font-bold tracking-tight">🍽️ Live Verified Dining Feed</h2>
      {logs.map((log: any, idx: number) => (
        <div key={idx} className="p-4 bg-white dark:bg-zinc-900 border border-zinc-200 dark:border-zinc-800 rounded-xl shadow-sm">
          <div className="flex justify-between items-center">
            <span className="font-semibold text-zinc-900 dark:text-zinc-100">
              @{log.attester}
            </span>
            <span className="px-2 py-0.5 text-xs bg-emerald-100 dark:bg-emerald-950 text-emerald-800 dark:text-emerald-300 rounded-full font-mono">
              ★ {((log.metadata?.overallRatingBps || 9000) / 1000).toFixed(1)} / 10.0
            </span>
          </div>
          <p className="mt-2 text-sm text-zinc-600 dark:text-zinc-400">
            Dined at Restaurant #{log.entityId} &bull; Verified via Receipt Hash
          </p>
          {log.metadata?.dishes && (
            <div className="mt-2 flex flex-wrap gap-1">
              {log.metadata.dishes.map((dish: string, i: number) => (
                <span key={i} className="text-xs px-2 py-0.5 bg-zinc-100 dark:bg-zinc-800 rounded text-zinc-700 dark:text-zinc-300">
                  {dish}
                </span>
              ))}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
```

---

## 7. Launch a Monetized Tastemaker Guild (The 93% Creator Moat)

Top food critics and influencers are leaving Beli because Beli does not pay them. On Muabase, tastemakers create exclusive Curation Guilds (e.g., *"NYC Natural Wine Syndicate"*, *"Tokyo Omakase Club"*). 

Subscribers pay a monthly fee, and **93% of revenue routes directly to the tastemaker**:

```typescript
// actions/createGuild.ts
import { client } from '@/lib/muabase';

export async function launchTastemakerGuild(params: {
  guildName: string;
  monthlyFeeCents: number;
  curatorBadge: string; // e.g. "sommelier_l3", "michelin_inspector"
}) {
  return await client.dining.guilds.createTastemakerGuild({
    name: params.guildName,
    curatorAccount: client.account,
    curatorDomainBadge: params.curatorBadge,
    monthlyMembershipCents: params.monthlyFeeCents,
    legalWrapperHash: '0x' + 'a'.repeat(64),
  });
}
```

### Fee Distribution Breakdown:
$$\text{Gross Membership Revenue} = 100\%$$
- **93% $\rightarrow$ Tastemaker / Curator:** Autonomous instant payout.
- **4% $\rightarrow$ Mutual Dining Shield (*Takaful* Pool):** Underwrites chef cancellations, bad reservation experiences, or fraudulent claims.
- **3% $\rightarrow$ Your App Platform:** Sustainable SaaS fee without advertising extraction.

---

## 8. Non-Custodial Supper Club & Group Reservation Escrow

Beli cannot handle commerce. Your app can host intimate Chef's Table dinners and private dining clubs. Payments are split non-custodially via Stripe Connect or native crypto with **zero operator custody liability**:

```typescript
// actions/bookSupperClub.ts
import { client } from '@/lib/muabase';

export async function reserveSupperClubSeats(params: {
  clubId: number;
  seats: number;
  pricePerSeatCents: number;
}) {
  const totalAmountCents = params.seats * params.pricePerSeatCents;

  // 1. Create dining usufruct lease directly on lease-eng Wasm service
  const lease = await client.leaseEngine.createLease({
    lessor: 'host-chef',
    tokenId: params.clubId,
    durationSec: 14400, // 4-hour dining usufruct
    grossPayment: { value: totalAmountCents * 100 },
    paymentTokenId: 1,  // USD stablecoin
    charitySinkAccount: 'waqf-sadaqah',
  });

  // 2. Dispatch non-custodial Stripe Connect multi-party split intent
  const split = await client.externalVault.generateSplitIntent({
    leaseId: lease.leaseId || 401,
    railType: 'stripe_connect',
    grossAmountCents: totalAmountCents,
    lenderAccount: 'host-chef',
  });

  console.log(`Booking confirmed: #${lease.leaseId}`);
  console.log(`Chef Payout (93%): $${((totalAmountCents * 0.93) / 100).toFixed(2)}`);
  console.log(`Mutual Shield Reserve (4%): $${((totalAmountCents * 0.04) / 100).toFixed(2)}`);
  console.log(`Platform Fee (3%): $${((totalAmountCents * 0.03) / 100).toFixed(2)}`);

  return { lease, split };
}
```

---

## 9. Zero-*Riba* Fair Cancellation Guard (Kitchen Restitution + Charity Sink)

Late cancellations and no-shows destroy restaurant economics, but Web2 platforms make enemies of their diners by pocketing 100% of cancellation penalties as platform profit.

Muabase hardcodes an ethical invariant:
1. **Direct Restitution:** The restaurant is compensated for actual perishable food and prep costs incurred.
2. **Punitive Deterrence:** Any additional penalty is routed 100% to a community food rescue charity sink (`charity_sink_account` / *Masraf al-Sadaqah*). Neither your platform nor the diner profits from misfortune:

```typescript
// actions/cancelBooking.ts
import { client } from '@/lib/muabase';

export async function cancelDiningReservation(params: {
  bookingId: string;
  actualKitchenCostCents: number; // Reimbursed to chef for food purchased
  punitiveLateFeeCents: number;   // Punitive penalty routed to charity sink
}) {
  // Settle via canonical leaseEngine service with zero-creditor enrichment
  return await client.leaseEngine.settleCancellation({
    leaseId: parseInt(params.bookingId, 10),
    actualKitchenRestitution: { value: params.actualKitchenCostCents * 100 },
    punitivePenalty: { value: params.punitiveLateFeeCents * 100 },
    charitySinkAccount: 'waqf-sadaqah', // Community food pantry / Masraf al-Sadaqah
  });
}
```

---

## 10. Dual-Locale Terminology Switcher

Support both modern consumer dining branding and authentic *Fiqh al-Mu‘amalat* terms with a single client toggle:

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
    <div className="flex items-center space-x-2">
      <button 
        onClick={toggle}
        className="px-3 py-1 text-xs font-medium rounded-lg bg-zinc-800 text-zinc-200 hover:bg-zinc-700 transition"
      >
        {locale === 'institutional' ? '🍴 Tastemaker Mode' : '⚖️ Fiqh al-Mu‘amalat Mode'}
      </button>
      <span className="text-xs text-zinc-400">
        Active: <code className="font-mono">{locale}</code>
      </span>
    </div>
  );
}
```

| Canonical Concept | Consumer / Institutional Locale (`"institutional"`) | Jurisprudential Locale (`"muamalat"`) |
| :--- | :--- | :--- |
| `asset_pool` | Restaurant Registry & Dining Guild | *Sukuk al-Qirad / Musharakah* |
| `lease` | Table Booking / Supper Club Lease | *Ijarah al-Manafi‘* |
| `mutual_shield` | Dining Protection Reserve Pool | *Takaful (Tabarru‘ & Ta‘awun)* |
| `claim_council` | Dining Dispute & Review Committee | *Majlis al-Tahkim & Shura* |
| `evals` | Pairwise Condorcet Restaurant Ranking | *Ju‘alah Halaqa* |
| `trust_score` | Diner Reliability Score | *‘Adalah (Probity / Reliability)* |
| `domain_badge` | Verified Tastemaker / Sommelier Badge | *Ikhtisas (Domain Specialization)* |
| `charity_sink` | Community Food Kitchen Sink | *Masraf al-Sadaqah* |
| `tangible_ratio` | Physical Meal & Hospitality Floor (100%) | *Nisbat al-‘Ayn al-Haqiqiyyah* |

---

## 11. Summary of Mathematical Guarantees & Regulatory Safe Harbor

1. **Zero Native Speculative Token:** No volatile gas or governance tokens. Bandwidth is provisioned via bonded compute capacity, insulating your app from SEC classification.
2. **NYDFS BitLicense Pure Software Exemption (`23 NYCRR § 200.2(q)`):** Your app provides non-custodial software interfaces to decentralized Wasm state machines and multi-party payment rails (Stripe Connect / Taproot / Safe). You hold **zero customer funds**.
3. **Sybil Resistance via Soulbound Identity:** `trust_score` and `domain_badge` are non-transferable and mathematically linked to cryptographic keys, preventing review farms and paid shill accounts from diluting rankings.
4. **Sub-Millisecond Wasm Speed:** All queries and mutations resolve directly against memory-mapped Trie databases (`Triedent`), giving your users the responsive feel of a local SQLite database with global BFT synchronization.

---

## Next Steps

- Clone the [Fashion Guild Quickstart](fashion-guild-quickstart.md) or [MizanVest Quickstart](mizanvest-quickstart.md) for related peer-to-peer and cooperative patterns.
- Read the complete protocol specification in [MUABASE_SPEC.md](../../../../doc/MUABASE_SPEC.md).
