import {
  MuabaseConfig,
  TerminologyLocale,
  CreateAssetInput,
  CheckoutLeaseInput,
  DamageClaimInput,
  UsufructLease,
  CoopPoolInput,
  RevenueReportInput,
  DistributionResult,
  CreateMarketInput,
  StakeAllocationInput,
  MarketPosition,
  MarketResolutionInput,
  ClaimRewardResult,
  RegisterRestaurantInput,
  ProofOfVisitInput,
  PairwiseComparisonInput,
  SupperClubBookingInput,
  SupperClubBookingResult,
  BookingCancellationInput,
  TastemakerGuildInput,
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

  /**
   * Real Estate & Infrastructure Digital Cooperative namespace (MizanVest)
   */
  public coop = {
    pools: {
      create: async (input: CoopPoolInput) => {
        return this.transact("tokens", "configureAssetPool", {
          config: {
            tokenId: 2001,
            legalWrapperHash: input.legalWrapperHash,
            maxHolders: 499,
            minTrustScore: 6000,
            tangibleRatioBps: input.tangibleRatioBps || 10000,
            priorityWindowSec: input.priorityWindowSec || 86400,
            custodianAccount: input.custodianAccount || "mizan-custodian",
            custodianFeeBps: 200, // 2% annual management fee
            charitySinkAccount: "waqf-sadaqah",
          },
        });
      },

      reportRevenue: async (input: RevenueReportInput) => {
        return this.transact("tokenstream", "recordGrossRevenue", {
          poolId: input.poolId,
          source: input.source,
          grossAmountCents: input.grossAmountCents,
          operatingExpensesCents: input.operatingExpensesCents,
          evidenceHash: input.evidenceHash,
        });
      },

      executeStabilizedDistribution: async (params: {
        poolId: number;
        grossRevenueCents: number;
        operatingExpensesCents: number;
        portfolioValuationCents: number;
      }): Promise<DistributionResult> => {
        const netIncomeCents = params.grossRevenueCents - params.operatingExpensesCents;
        const grossYieldBps = Math.floor((params.grossRevenueCents * 10000) / params.portfolioValuationCents);
        const netYieldBps = Math.floor((netIncomeCents * 10000) / params.portfolioValuationCents);

        let effectivePayoutBps = netYieldBps;
        let reserveSurplusAddedCents = 0;
        let reserveDrawCents = 0;

        if (netYieldBps > 1200) {
          // Outperformance: Cap distribution at 12%, excess routes to Yield Stabilization Fund
          effectivePayoutBps = 1200;
          reserveSurplusAddedCents = Math.floor(((netYieldBps - 1200) * params.portfolioValuationCents) / 10000);
        } else if (netYieldBps < 800) {
          // Underperformance: Draw from Yield Stabilization Fund up to 8% target floor
          reserveDrawCents = Math.floor(((800 - netYieldBps) * params.portfolioValuationCents) / 10000);
          effectivePayoutBps = 800;
        }

        const res = await this.transact("tokenstream", "distributeCoopYield", {
          poolId: params.poolId,
          netYieldBps,
          effectivePayoutBps,
          reserveSurplusAddedCents,
          reserveDrawCents,
        });

        return {
          distributionId: String(res.distributionId || "dist-1"),
          poolId: params.poolId,
          grossYieldBps,
          netYieldBps,
          effectivePayoutBps,
          reserveSurplusAddedCents,
          reserveDrawCents,
          reserveTotalBalanceCents: res.reserveTotalBalanceCents || 25000000,
        };
      },
    },

    shares: {
      invest: async (input: { poolId: number; amountCents: number; rail?: string }) => {
        if (input.rail === "stripe_connect") {
          return await this.transact("ext-vault", "generateInvestmentIntent", {
            poolId: input.poolId,
            amountCents: input.amountCents,
            railType: "stripe_connect",
            investorAccount: this.account,
          });
        }

        return await this.transact("tokens", "purchaseDigitalShares", {
          poolId: input.poolId,
          amountCents: input.amountCents,
          buyer: this.account,
        });
      },
    },
  };

  /**
   * Ethical Forecasting & Ju'alah Knowledge Bounty namespace (Irfan.Markets)
   */
  public forecasting = {
    markets: {
      create: async (input: CreateMarketInput) => {
        return this.transact("evals", "createJu'alahBounty", {
          question: input.question,
          category: input.category,
          outcomes: input.outcomes,
          resolutionTimestamp: input.resolutionTimestamp,
          oracleAccount: input.oracleAccount,
          minTrustScore: input.minTrustScore || 5000,
          descriptionHash: input.descriptionHash,
          charitySinkAccount: "waqf-sadaqah",
        });
      },

      resolve: async (input: MarketResolutionInput) => {
        return this.transact("evals", "resolveJu'alahBounty", {
          marketId: input.marketId,
          winningOutcomeIndex: input.winningOutcomeIndex,
          evidenceHash: input.evidenceHash,
          dualWitnessSignatures: input.dualWitnessSignatures || [],
        });
      },
    },

    vault: {
      allocateStake: async (input: StakeAllocationInput): Promise<MarketPosition> => {
        const result = await this.transact("tokenstream", "allocateYieldStake", {
          marketId: input.marketId,
          outcomeIndex: input.outcomeIndex,
          principalAmountUsd: input.principalAmountUsd,
          staker: this.account,
          rail: input.rail || "native_usdc",
        });

        return {
          positionId: String(result.positionId || "pos-1"),
          marketId: input.marketId,
          account: this.account,
          outcomeIndex: input.outcomeIndex,
          principalAmountUsd: input.principalAmountUsd,
          prizeShares: input.principalAmountUsd * 86400, // Amount * Duration (stake_seconds)
          allocationTimestamp: Math.floor(Date.now() / 1000),
          status: "Active",
        };
      },

      claim: async (positionId: string): Promise<ClaimRewardResult> => {
        const res = await this.transact("tokenstream", "claimJu'alahReward", {
          positionId: parseInt(positionId, 10),
          claimer: this.account,
        });

        return {
          positionId,
          principalRefundUsd: res.principalRefundUsd, // 100% of user principal returned
          yieldPrizeRewardUsd: res.yieldPrizeRewardUsd || 0, // Pro-rata share of redirected yield
          skillPointsEarned: res.skillPointsEarned || 250, // Soulbound trust_score addition
        };
      },
    },
  };

  /**
   * Social Restaurant Tracking & Tastemaker Guild namespace (Beli Challenger)
   */
  public dining = {
    restaurants: {
      register: async (input: RegisterRestaurantInput) => {
        return this.transact("tokens", "configureAssetPool", {
          config: {
            tokenId: 3001,
            legalWrapperHash: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
            maxHolders: 10000,
            minTrustScore: input.minTrustScore || 6500,
            tangibleRatioBps: input.tangibleRatioBps || 10000,
            priorityWindowSec: 86400,
            custodianAccount: "restaurant-registry",
            custodianFeeBps: 0,
            charitySinkAccount: "waqf-sadaqah",
          },
        });
      },

      submitProofOfVisit: async (input: ProofOfVisitInput) => {
        return this.transact("staged-tx", "recordAttestation", {
          service: "evaluation",
          entityId: input.restaurantId,
          evidenceHash: input.receiptDigestHash,
          attester: input.dinerAccount || this.account,
          witnessQuorum: input.witnessAccounts?.length || 1,
          metadata: {
            overallRatingBps: input.overallRatingBps,
            dishes: input.dishesOrdered,
            timestamp: input.visitTimestamp,
          },
        });
      },
    },

    rankings: {
      recordComparison: async (input: PairwiseComparisonInput) => {
        return this.transact("evals", "recordPairwiseVote", {
          winnerId: input.winnerRestaurantId,
          loserId: input.loserRestaurantId,
          category: input.category || "overall",
          voter: this.account,
          receiptHash: input.receiptDigestHash || null,
        });
      },
    },

    supperClubs: {
      book: async (input: SupperClubBookingInput): Promise<SupperClubBookingResult> => {
        const totalAmountCents = input.seats * input.pricePerSeatCents;
        const result = await this.transact("lease-eng", "createLease", {
          lessor: "host-chef",
          tokenId: input.clubId,
          nftId: null,
          durationSec: 14400, // 4-hour dining usufruct
          grossPayment: { value: totalAmountCents * 100 },
          paymentTokenId: 1,
          preConditionEvidenceHash: input.preConditionEvidenceHash || "0x0",
          isProgressiveBuyout: false,
          buyoutEquityUnits: { value: 0 },
          custodianAccount: "supper-host",
          custodianFeeBps: 0,
          charitySinkAccount: "waqf-sadaqah",
        });

        if (input.rail === "stripe_connect") {
          await this.transact("ext-vault", "generateSplitIntent", {
            leaseId: result.leaseId || 401,
            railType: "stripe_connect",
            grossAmountCents: totalAmountCents,
            lenderAccount: "host-chef",
          });
        }

        return {
          bookingId: String(result.leaseId || 401),
          clubId: input.clubId,
          diner: this.account,
          seats: input.seats,
          totalAmountCents,
          status: "Confirmed",
          splits: {
            hostTakeHomeCents: Math.floor(totalAmountCents * 0.93),
            mutualShieldReserveCents: Math.floor(totalAmountCents * 0.04),
            platformFeeCents: Math.floor(totalAmountCents * 0.03),
          },
        };
      },

      cancelWithEthicalDeterrence: async (input: BookingCancellationInput) => {
        return this.transact("lease-eng", "settleCancellation", {
          leaseId: parseInt(input.bookingId, 10),
          actualKitchenRestitution: { value: input.actualKitchenCostCents * 100 },
          punitivePenalty: { value: input.punitiveLateFeeCents * 100 },
          charitySinkAccount: "waqf-sadaqah",
        });
      },
    },

    guilds: {
      createTastemakerGuild: async (input: TastemakerGuildInput) => {
        return this.transact("guilds", "createPod", {
          name: input.name,
          curator: input.curatorAccount,
          requiredDomainBadge: input.curatorDomainBadge,
          membershipFeeCents: input.monthlyMembershipCents,
          splits: {
            curatorBps: 9300,
            mutualShieldBps: 400,
            platformBps: 300,
          },
        });
      },
    },
  };
}

export function createMuabaseClient(config: MuabaseConfig): MuabaseClient {
  return new MuabaseClient(config);
}
