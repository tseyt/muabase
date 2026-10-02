# Quickstart: Build a Sharia-Compliant Real Estate & Infrastructure Cooperative on Muabase (15 Minutes)

This quickstart guides you through building a high-yield, transparent real-asset digital cooperative—pooling **Workforce Housing** and **Commercial Solar** infrastructure—using `@muabase/headless-sdk`. Inspired by [MizanVest](https://tseyt.gitlab.io/mizanvest), this architecture delivers predictable quarterly income with cryptographic verification and zero gas fees for investors.

Key architectural advantages:
- **Glass Vault Serverless Transparency:** 100% of application interfaces, financial logic, and property ledgers execute directly on-chain via Psibase Wasm-in-Database (`Sites`), eliminating phishing vectors and black-box databases.
- **Yield Stabilization Fund (*Sanduq al-Mizan / Himayah*):** Mathematical distribution smoothing that absorbs volatile asset cash flows (historical 14–35% yields from rent and utility solar contracts) into a predictable **8–12% annual distribution**. Excess yield above 12% is automatically routed to a community-owned reserve fund (capped at 5% of total portfolio value); deficits below 8% draw from this reserve.
- **True Risk-Sharing, Zero-Debt Guarantees:** In accordance with classical *Musharakah* equity principles, if the Yield Stabilization Fund is depleted, investors receive actual operational returns rather than manufactured debt promises.
- **Frictionless Zero-Gas Transactions:** Psibase computational CPU limit staking allows the Cooperative to absorb network execution costs at the protocol level—investors check balances, view transactions, and receive distributions without micro-gas fee friction.
- **100% Tangible Asset Floor (*Bay‘ al-Dayn* Invariant):** Digital shares directly represent physical real estate (*A‘yan*) and solar usufruct rights (*Manafi‘*). The protocol enforces `tangible_ratio_bps >= 5100` (default 10,000 / 100%), barring naked debt speculation or discounted secondary debt trading.
- **Co-Owner Right of First Refusal (*Haqq al-Shuf‘ah*):** Fractional co-owners enjoy an automated pre-emption window (`priority_window_sec: 86400`, 24 hours) before secondary shares can be traded to external parties.
- **Dual-Locale Terminology:** Seamlessly toggle between commercial institutional terms ("Digital Shares", "Yield Stabilization Reserve") and authentic *Fiqh al-Mu‘amalat* terms (*Musharakah*, *Sukuk al-Ijarah*, *Sanduq al-Himayah*).

---

## 1. Installation

Install the headless SDK and its peer dependencies in your Next.js, Vite, or React project:

```bash
npm install @muabase/headless-sdk
```

---

## 2. Initialize the Client

Configure the client to connect directly to your local `psinode` or testnet RPC gateway. No centralized backend or intermediate API middleware is required:

```typescript
// lib/muabase.ts
import { createMuabaseClient } from '@muabase/headless-sdk';

export const client = createMuabaseClient({
  endpoint: process.env.NEXT_PUBLIC_PSIBASE_RPC_URL || 'http://localhost:8080',
  locale: 'institutional', // Toggle between 'institutional' and 'muamalat'
  account: 'investor1',
});
```

---

## 3. Syndicate an Infrastructure Asset Pool (Sponsor Flow)

Cooperative sponsors register real physical assets—such as workforce housing townhomes in Indiana/Florida/Texas and commercial solar arrays with 20-year utility Power Purchase Agreements (PPAs)—into an `AssetPool`:

```typescript
// actions/createPool.ts
import { client } from '@/lib/muabase';

export async function createInfrastructurePool(params: {
  name: string;
  legalDeedHash: string; // SHA-256 of Master Series LLC / Trust agreement
}) {
  return await client.coop.pools.create({
    name: params.name,
    targetYieldMinBps: 800,           // 8.00% annual distribution floor
    targetYieldMaxBps: 1200,          // 12.00% annual distribution cap
    stabilizationReserveCapBps: 500,  // Reserve capped at 5.00% of portfolio valuation
    tangibleRatioBps: 10000,          // 100% tangible asset backing (no debt packaging)
    priorityWindowSec: 86400,         // 24h co-owner priority window (Haqq al-Shuf'ah)
    legalWrapperHash: params.legalDeedHash,
    custodianAccount: 'mizan-custodian',
  });
}
```

---

## 4. Real-Time Glass Vault Treasury Hook

Use `useMuabaseTable` to subscribe directly to real-time treasury inflows, asset valuations, and stabilization reserve balances. Data updates reactively via Server-Sent Events (SSE) from memory-mapped Trie storage (`Triedent`):

```tsx
// components/TreasuryDashboard.tsx
'use client';

import React from 'react';
import { useMuabaseTable } from '@muabase/headless-sdk';
import { client } from '@/lib/muabase';

export function TreasuryDashboard() {
  const { data: records, loading, error } = useMuabaseTable({
    client,
    service: 'tokenstream',
    table: 'RevenueRecordTable',
    query: '{ revenueRecordTable { poolId source grossAmountCents netYieldBps timestamp } }',
    pollIntervalMs: 4000,
  });

  if (loading) return <div>Auditing Glass Vault Treasury...</div>;
  if (error) return <div>Treasury Audit Error: {error.message}</div>;

  return (
    <div className="p-6 bg-slate-900 text-white rounded-xl shadow-lg border border-slate-800">
      <h2 className="text-xl font-bold mb-4">🏛️ Glass Vault: Real-Time Cashflow & Audits</h2>
      <div className="space-y-3">
        {records.map((r: any, idx: number) => (
          <div key={idx} className="flex justify-between items-center p-3 bg-slate-800 rounded border border-slate-700">
            <div>
              <span className="font-semibold capitalize">{r.source.replace(/_/g, ' ')}</span>
              <p className="text-xs text-gray-400">Recorded: {new Date(r.timestamp * 1000).toLocaleDateString()}</p>
            </div>
            <div className="text-right">
              <span className="text-emerald-400 font-mono font-bold">
                +${(r.grossAmountCents / 100).toLocaleString()}
              </span>
              <p className="text-xs text-gray-400">Net Yield: {(r.netYieldBps / 100).toFixed(2)}%</p>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
```

---

## 5. Acquiring Digital Shares (Investor Flow)

Members acquire fractional co-op shares via non-custodial fiat rails (Stripe Connect custom splits) or on-chain USD stablecoins. Network execution fees are absorbed by the Cooperative's CPU quota:

```typescript
// actions/invest.ts
import { client } from '@/lib/muabase';

export async function investInCooperative(poolId: number, amountUsd: number) {
  const amountCents = amountUsd * 100;

  // Dispatches investment intent without intermediary key custody
  const result = await client.coop.shares.invest({
    poolId,
    amountCents,
    rail: 'stripe_connect',
  });

  console.log(`Digital Shares Issued: Pool #${poolId}, Value: $${amountUsd}`);
  return result;
}
```

---

## 6. Immutable Revenue Reporting (Property & Solar Operators)

Property managers and utility solar operators log gross receipts directly to the immutable ledger alongside cryptographic digest proofs (utility meter hashes, tenant payment receipts):

```typescript
// actions/reportRevenue.ts
import { client } from '@/lib/muabase';

export async function logQuarterlyRevenue(params: {
  poolId: number;
  source: 'workforce_housing_rent' | 'commercial_solar_ppa';
  grossAmountUsd: number;
  operatingExpensesUsd: number;
  evidenceHash: string; // SHA-256 of utility statement or rent roll
}) {
  return await client.coop.pools.reportRevenue({
    poolId: params.poolId,
    source: params.source,
    grossAmountCents: params.grossAmountUsd * 100,
    operatingExpensesCents: params.operatingExpensesUsd * 100,
    evidenceHash: params.evidenceHash,
  });
}
```

---

## 7. Automated Yield Stabilization Distribution Flow

At the close of each quarter, the distribution protocol computes net operational yield and executes the stabilization formula:

$$\text{Net Yield} = \frac{\text{Rent} + \text{Solar Revenue} - \text{Operating Expenses}}{\text{Portfolio Valuation}}$$

$$\text{Distribution} = \begin{cases}
12\% & \text{if Net Yield} > 12\% \quad (\text{Surplus} \longrightarrow \text{Stabilization Fund}) \\
\text{Net Yield} & \text{if } 8\% \le \text{Net Yield} \le 12\% \\
\min(8\%, \text{Net Yield} + \text{Fund Draw}) & \text{if Net Yield} < 8\%
\end{cases}$$

```typescript
// actions/executeDistribution.ts
import { client } from '@/lib/muabase';

export async function runQuarterlyDistribution(params: {
  poolId: number;
  grossRevenueUsd: number;
  operatingExpensesUsd: number;
  portfolioValuationUsd: number;
}) {
  // Evaluates and applies the 8-12% stabilization formula
  const distribution = await client.coop.pools.executeStabilizedDistribution({
    poolId: params.poolId,
    grossRevenueCents: params.grossRevenueUsd * 100,
    operatingExpensesCents: params.operatingExpensesUsd * 100,
    portfolioValuationCents: params.portfolioValuationUsd * 100,
  });

  console.log(`Quarterly Payout: ${(distribution.effectivePayoutBps / 100).toFixed(2)}%`);
  if (distribution.reserveSurplusAddedCents > 0) {
    console.log(`Outperformance Surplus Added to Reserve: $${distribution.reserveSurplusAddedCents / 100}`);
  }
  if (distribution.reserveDrawCents > 0) {
    console.log(`Underperformance Supplement Drawn from Reserve: $${distribution.reserveDrawCents / 100}`);
  }

  return distribution;
}
```

---

## 8. Dual-Locale Terminology Switcher

Support both institutional commercial terminology and classical *Fiqh al-Mu‘amalat* terms dynamically:

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
    <div className="flex items-center space-x-3 p-3 bg-slate-800 rounded-lg">
      <button 
        onClick={toggle}
        className="px-4 py-2 text-sm font-semibold bg-teal-600 hover:bg-teal-500 text-white rounded transition"
      >
        Mode: {locale === 'institutional' ? '🏢 Institutional Cooperative' : '⚖️ Fiqh al-Mu‘amalat'}
      </button>

      <span className="text-xs text-slate-400">
        Active Terms: {locale === 'institutional' 
          ? 'Digital Shares • Yield Stabilization Fund • Co-Owner Pre-Emption' 
          : 'Sukuk al-Ijarah • Sanduq al-Mizan • Haqq al-Shuf‘ah'}
      </span>
    </div>
  );
}
```

---

## 9. Summary of Mathematical Guarantees

| Invariant | Canonical Key | Contract / Wasm Mechanism | Regulatory / Jurisprudential Reference |
| :--- | :--- | :--- | :--- |
| **Yield Stabilization Range** | `mutual_shield` / `TokenStream` | Cap at 1200 bps; Floor at 800 bps; Reserve capped at 500 bps (5%) | *Ta‘awun* (Mutual Cooperative Assistance); OIC Fiqh Academy Resolution 87 |
| **No-Debt Guarantee** | `endowment_policy` | If reserve is empty, distribution equals actual net yield | Prohibition of guaranteed equity returns (*Riba*); AAOIFI Shariah Std 12 (Sharika) |
| **100% Tangible Asset Floor** | `tangible_ratio` | Non-par trades blocked if `tangible_ratio_bps < 5100` (100% physical) | AAOIFI Shariah Std 21 (Sukuk); Bay‘ al-Dayn Prohibition |
| **Co-Owner Pre-Emption** | `priority_window` | 24h exclusive co-owner order fill window (`rofr_window_sec: 86400`) | Classical *Haqq al-Shuf‘ah* Pre-emption Right |
| **Glass Vault Transparency** | `Sites` | Complete frontend SPA and contract bytecode hosted on-ledger | Anti-Phishing Safe Harbor; Immutable Source of Truth |
| **Zero Gas Friction** | `cpu_limit` / `Permissions` | Network bandwidth covered via Cooperative CPU staking | Pure Software Exemption; NYC BitLicense `23 NYCRR § 200.2(q)` |
