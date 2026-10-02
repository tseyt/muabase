# Quickstart: Build an Ethical Positive-Sum Forecasting Protocol on Muabase (15 Minutes)

This quickstart guides you through building a principal-protected, positive-sum forecasting and prediction market protocol (outcompeting speculative zero-sum casinos like Polymarket) using `@muabase/headless-sdk`. Inspired by [Irfan.Markets](https://tseyt.gitlab.io/irfan-markets), this architecture rewards deep analytical research while guaranteeing zero gambling loss (*Maysir*).

Key architectural advantages:
- **100% Principal Protection (Zero-*Maysir* Invariant):** Forecasters never risk their capital. Deposited funds sit in a **Unified Yield Vault** (`endowment_policy`) earning native staking or RWA yield (e.g., Marinade mSOL, tokenized T-Bills, or low-risk cooperative yield). When a market resolves, 100% of principal is returned to all participants.
- **Ju‘alah Reward Structure (*Ju‘alah al-Irfan*):** Rewards are framed legally and jurisprudentially as performance bounties (*Ju‘alah*) derived exclusively from redirected *yield*, never from other contestants' forfeited losses.
- **Time-Weighted Conviction (Prize Shares):** Early and steadfast predictions accrue higher reward weight via $\text{Prize Shares} = \text{Principal} \times \text{Duration}$ ($\text{stake\_seconds}$), neutralizing last-minute speculative front-running.
- **Zero-House Extraction & Charity Surplus Sink:** Unclaimed or unwon yield is never retained by the protocol operator; 100% of surplus is routed to a community charity sink (`charity_sink_account` / *Masraf al-Sadaqah*).
- **Binary *Bayyinah* Resolution Gate:** Disputed or complex market outcomes require verification by an authorized oracle or a 2-witness quorum holding verified reliability (`trust_score` / `AdalahScore`) and domain expertise (`domain_badge` / `IkhtisasTag`).
- **Soulbound Skill Points:** Accurate forecasters accumulate non-transferable reputation credentials rather than speculative governance tokens, establishing an auditable track record for institutional knowledge workers.
- **Dual-Locale Terminology:** Seamlessly toggle between commercial forecasting labels and classical *Fiqh al-Mu‘amalat* terms.

---

## 1. Installation

Install the headless SDK and its peer dependencies in your Next.js, Vite, or React Native project:

```bash
npm install @muabase/headless-sdk
```

---

## 2. Initialize the Client

Connect directly to your local `psinode` or testnet RPC gateway without backend middleware:

```typescript
// lib/muabase.ts
import { createMuabaseClient } from '@muabase/headless-sdk';

export const client = createMuabaseClient({
  endpoint: process.env.NEXT_PUBLIC_PSIBASE_RPC_URL || 'http://localhost:8080',
  locale: 'institutional', // or 'muamalat'
  account: 'forecaster1',
});
```

---

## 3. Create an Ethical Forecasting Market (Creator Flow)

Market creators post objective questions with deterministic resolution parameters, linking Ricardian description digests and designating authorized oracles:

```typescript
// actions/createMarket.ts
import { client } from '@/lib/muabase';

export async function createEthicalPredictionMarket(params: {
  question: string;
  category: string;
  outcomes: string[];
  resolutionTimestamp: number;
  descriptionHash: string; // SHA-256 of resolution criteria documentation
}) {
  return await client.forecasting.markets.create({
    question: params.question,
    category: params.category,
    outcomes: params.outcomes,
    resolutionTimestamp: params.resolutionTimestamp,
    oracleAccount: 'oracle-service',
    minTrustScore: 5000, // Minimum 50.00% trust score required to participate
    descriptionHash: params.descriptionHash,
  });
}
```

---

## 4. Real-Time Market Board Hook

Use `useMuabaseTable` to subscribe to active forecasting questions, total allocated principal, and the accrued yield prize pool. Data streams reactively via Server-Sent Events (SSE) directly from memory-mapped Trie storage (`Triedent`):

```tsx
// components/MarketBoard.tsx
'use client';

import React from 'react';
import { useMuabaseTable } from '@muabase/headless-sdk';
import { client } from '@/lib/muabase';

export function MarketBoard() {
  const { data: markets, loading, error } = useMuabaseTable({
    client,
    service: 'evals',
    table: 'Ju'alahBountyTable',
    query: '{ jualahBountyTable { marketId question outcomes totalStakedUsd accruedYieldUsd status } }',
    filter: (m: any) => m.status === 'Open',
    pollIntervalMs: 3000,
  });

  if (loading) return <div>Loading active prediction markets...</div>;
  if (error) return <div>Failed to load markets: {error.message}</div>;

  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-6 p-4">
      {markets.map((m: any) => (
        <div key={m.marketId} className="bg-slate-900 border border-slate-800 rounded-xl p-5 text-white shadow-md">
          <span className="text-xs font-mono text-teal-400">Market #{m.marketId}</span>
          <h3 className="text-lg font-bold mt-1">{m.question}</h3>
          
          <div className="mt-4 flex justify-between text-sm text-slate-300">
            <div>
              <p className="text-xs text-slate-500">Principal Locked (Safe)</p>
              <p className="font-semibold font-mono">${m.totalStakedUsd.toLocaleString()}</p>
            </div>
            <div className="text-right">
              <p className="text-xs text-amber-400 font-medium">✨ Yield Prize Pool</p>
              <p className="font-bold font-mono text-amber-300">${m.accruedYieldUsd.toFixed(2)}</p>
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}
```

---

## 5. Allocate Stake to an Outcome (Forecaster Flow)

Forecasters deposit principal into the Unified Yield Vault. Their principal remains 100% intact while redirecting generated yield into the prize pool. The user immediately begins accruing time-weighted **Prize Shares**:

```typescript
// actions/allocateStake.ts
import { client } from '@/lib/muabase';

export async function stakeConviction(params: {
  marketId: number;
  outcomeIndex: number;
  principalAmountUsd: number;
}) {
  // Principal is locked in the Unified Yield Vault with zero risk of capital loss
  const position = await client.forecasting.vault.allocateStake({
    marketId: params.marketId,
    outcomeIndex: params.outcomeIndex,
    principalAmountUsd: params.principalAmountUsd,
    rail: 'solana_marinade', // External liquid staking yield bridge
  });

  console.log(`Position Created! ID: ${position.positionId}, Accruing Prize Shares...`);
  return position;
}
```

---

## 6. Cross-Chain Yield Bridge & External Vault Intent

When funding stakes via external liquid staking assets (e.g. Marinade mSOL on Solana or Compound USDC on EVM), an `ExternalVault` intent is dispatched without custodial intermediary keys:

```typescript
// actions/externalVaultStake.ts
import { client } from '@/lib/muabase';

export async function bridgeStakingYield(marketId: number, solAmount: number) {
  // Generates non-custodial Miniscript / Solana Squads deposit instruction
  return await client.transact('ext-vault', 'generateYieldDepositIntent', {
    marketId,
    sourceChain: 'solana_mainnet',
    yieldProvider: 'marinade_msol',
    depositAmountLamports: solAmount * 1e9,
    beneficiaryAccount: client.account,
  });
}
```

---

## 7. Market Resolution & Zero-Risk Claim Flow

When the market concludes, the outcome is verified via an authorized oracle or a 2-witness *Bayyinah* evidentiary gate (`claim_council`):

### Market Resolution
```typescript
// actions/resolveMarket.ts
import { client } from '@/lib/muabase';

export async function resolveMarket(params: {
  marketId: number;
  winningOutcomeIndex: number;
  evidenceHash: string; // Cryptographic hash of official event record
  dualWitnessSignatures: string[]; // Bayyinah: 2 verified witnesses
}) {
  return await client.forecasting.markets.resolve({
    marketId: params.marketId,
    winningOutcomeIndex: params.winningOutcomeIndex,
    evidenceHash: params.evidenceHash,
    dualWitnessSignatures: params.dualWitnessSignatures,
  });
}
```

### Claiming Principal & Yield Bounties
```typescript
// actions/claimReward.ts
import { client } from '@/lib/muabase';

export async function claimForecastingReward(positionId: string) {
  // Guarantees 100% principal return; adds pro-rata yield if prediction was correct
  const result = await client.forecasting.vault.claim(positionId);

  console.log(`Principal Returned (100%): $${result.principalRefundUsd}`);
  if (result.yieldPrizeRewardUsd > 0) {
    console.log(`Ju'alah Yield Prize Won: $${result.yieldPrizeRewardUsd}`);
    console.log(`Skill Points Earned: +${result.skillPointsEarned} AdalahScore`);
  } else {
    console.log(`Prediction incorrect. Principal fully preserved. Zero capital lost.`);
  }

  return result;
}
```

---

## 8. Dual-Locale Terminology Switcher

Switch dynamically between mainstream institutional terms and classical *Fiqh al-Mu‘amalat* terms:

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
        className="px-4 py-2 text-sm font-semibold bg-amber-600 hover:bg-amber-500 text-white rounded transition"
      >
        Mode: {locale === 'institutional' ? '📊 Institutional Forecasting' : '⚖️ Fiqh al-Mu‘amalat'}
      </button>

      <span className="text-xs text-slate-400">
        Active Terms: {locale === 'institutional' 
          ? 'Principal-Protected Vault • Prize Shares • Time-Weighted Conviction' 
          : 'Hifz al-Asl • Ju‘alah al-Irfan • Bayyinah Resolution'}
      </span>
    </div>
  );
}
```

---

## 9. Summary of Mathematical Guarantees

| Invariant | Canonical Key | Contract / Wasm Mechanism | Regulatory / Jurisprudential Reference |
| :--- | :--- | :--- | :--- |
| **Zero-*Maysir* (No Gambling)** | `endowment_policy` | 100% principal preservation lock; principal can never be forfeit | Qur'an 5:90–91; Prohibition of zero-sum wagering (*Qimar*) |
| **Ju‘alah Reward Legality** | `evals` / `Ju'alah` | Prizes funded strictly from productive yield, never participant stakes | AAOIFI Shariah Std 15 (Ju‘alah); Performance wage validity |
| **Time-Weighted Conviction** | `evals` | $\text{Prize Shares} = \text{Principal} \times \text{Duration}$ ($\text{stake\_seconds}$) | Neutralizing speculative front-running & rewarding early research |
| **Zero House Extraction** | `charity_sink` | 100% of unwon yield routed to `charity_sink_account` | Qur'an 2:278–279; Absolute bar on predatory bookmaking profit |
| **Binary *Bayyinah* Gate** | `claim_council` / `evals` | 2-witness quorum (`trust_score` + `domain_badge`) for disputed resolutions | Qur'an 2:282; AAOIFI Shariah Std 5 (Guarantees & Testimony) |
| **Non-Monetizable Skill** | `trust_score` / `domain_badge` | Non-transferable `AdalahScore` & `IkhtisasTag` records | Pure software exemption; NYC BitLicense `23 NYCRR § 200.2(q)` |
