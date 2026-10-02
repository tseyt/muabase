import {
  MuabaseConfig,
  TerminologyLocale,
  CreateAssetInput,
  CheckoutLeaseInput,
  DamageClaimInput,
  UsufructLease,
} from "./types";

export class MuabaseClient {
  public endpoint: string;
  public locale: TerminologyLocale;
  public account: string;

  constructor(config: MuabaseConfig) {
    this.endpoint = config.endpoint.replace(/\/$/, "");
    this.locale = config.locale || "institutional";
    this.account = config.account || "anonymous";
  }

  /**
   * Raw GraphQL query runner directly against psinode Triedent DB
   */
  public async query<T = any>(service: string, queryStr: string, variables?: Record<string, any>): Promise<T> {
    const url = `${this.endpoint}/api/${service}/graphql`;
    const res = await fetch(url, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ query: queryStr, variables }),
    });

    if (!res.ok) {
      throw new Error(`Muabase Query Failed [${res.status}]: ${res.statusText}`);
    }

    const json = await res.json();
    if (json.errors && json.errors.length > 0) {
      throw new Error(`GraphQL Error: ${json.errors[0].message}`);
    }
    return json.data;
  }

  /**
   * Action submission to psinode via Transact / AuthSig passkeys
   */
  public async transact(service: string, method: string, args: Record<string, any>): Promise<any> {
    const url = `${this.endpoint}/api/${service}/action/${method}`;
    const res = await fetch(url, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(args),
    });

    if (!res.ok) {
      const err = await res.text();
      throw new Error(`Transaction Execution Reverted: ${err}`);
    }

    return await res.json();
  }

  /**
   * Guild domain namespace
   */
  public guild = {
    assets: {
      create: async (input: CreateAssetInput) => {
        return this.transact("tokens", "configureAssetPool", {
          config: {
            tokenId: 1042,
            legalWrapperHash: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
            maxHolders: input.maxHolders || 99,
            minTrustScore: input.minTrustScore ?? input.minAdalahScore ?? 7000,
            tangibleRatioBps: input.tangibleRatioBps || 10000,
            priorityWindowSec: input.priorityWindowSec ?? input.rofrWindowSec ?? 86400,
            custodianAccount: input.custodianAccount || "custodian",
            custodianFeeBps: input.custodianFeeBps || 0,
            charitySinkAccount: "waqf-sadaqah",
          },
        });
      },
    },

    leases: {
      checkout: async (input: CheckoutLeaseInput): Promise<UsufructLease> => {
        const grossPayment = (input.durationDays * 45) * 10000; // $45/day in precision 4
        const result = await this.transact("lease-eng", "createLease", {
          lessor: "asset-owner",
          tokenId: input.assetId,
          nftId: null,
          durationSec: input.durationDays * 86400,
          grossPayment: { value: grossPayment },
          paymentTokenId: 1, // USD Stablecoin / USDC
          preConditionEvidenceHash: input.preConditionEvidenceHash,
          isProgressiveBuyout: input.isProgressiveBuyout || false,
          buyoutEquityUnits: { value: input.buyoutEquityUnits || 0 },
          custodianAccount: "custodian",
          custodianFeeBps: 0,
          charitySinkAccount: "waqf-sadaqah",
        });

        // If paying via mainstream Stripe Connect fiat rail, trigger non-custodial destination split intent
        if (input.rail === "stripe_connect") {
          await this.transact("ext-vault", "generateSplitIntent", {
            leaseId: result.leaseId || 101,
            railType: "stripe_connect",
            grossAmountCents: (input.durationDays * 45) * 100, // cents
            lenderAccount: "asset-owner",
          });
        }

        return {
          leaseId: String(result.leaseId || 101),
          lessor: "asset-owner",
          lessee: this.account,
          tokenId: input.assetId,
          durationSec: input.durationDays * 86400,
          grossPayment,
          status: "Active",
          preConditionEvidenceHash: input.preConditionEvidenceHash,
          isProgressiveBuyout: input.isProgressiveBuyout || false,
          buyoutEquityUnits: input.buyoutEquityUnits || 0,
        };
      },

      confirmReturn: async (input: { leaseId: string; postConditionEvidenceHash: string }) => {
        return this.transact("lease-eng", "confirmCleanReturn", {
          leaseId: parseInt(input.leaseId, 10),
          postConditionEvidenceHash: input.postConditionEvidenceHash,
        });
      },
    },

    claims: {
      submitEvidence: async (input: DamageClaimInput) => {
        return this.transact("claimcouncil", "submitDamageClaim", {
          leaseId: parseInt(input.leaseId, 10),
          respondent: "renter-account",
          poolId: 1,
          claimedAmount: { value: input.claimedAmountUsd * 10000 },
          preConditionEvidenceHash: input.preConditionEvidenceHash,
          postConditionEvidenceHash: input.postConditionEvidenceHash,
          repairQuoteHash: input.repairQuoteHash,
        });
      },

      voteTriage: async (input: { claimId: string; approve: boolean; payoutUsd: number }) => {
        return this.transact("claimcouncil", "voteDamageClaim", {
          claimId: parseInt(input.claimId, 10),
          approve: input.approve,
          approvedPayout: { value: input.payoutUsd * 10000 },
        });
      },
    },

    credit: {
      netInvoices: async (invoice1: string, invoice2: string) => {
        return this.transact("creditclear", "executeBilateralNetting", {
          invoice1Id: parseInt(invoice1, 10),
          invoice2Id: parseInt(invoice2, 10),
        });
      },
    },
  };
}

export function createMuabaseClient(config: MuabaseConfig): MuabaseClient {
  return new MuabaseClient(config);
}
