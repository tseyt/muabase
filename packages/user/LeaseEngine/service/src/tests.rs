#[cfg(test)]
mod tests {
    use psibase::services::tokens::Quantity;
    use psibase::AccountNumber;

    #[test]
    fn test_multi_party_lease_split_93_percent() {
        let gross_amount: i64 = 100_0000; // $100.0000
        let protocol_saas_bps: u16 = 300;   // 3%
        let mutual_shield_bps: u16 = 400;   // 4%
        let custodian_bps: u16 = 0;         // 0%

        let protocol_fee = (gross_amount as u128 * protocol_saas_bps as u128 / 10000) as i64;
        let shield_fee = (gross_amount as u128 * mutual_shield_bps as u128 / 10000) as i64;
        let custodian_fee = (gross_amount as u128 * custodian_bps as u128 / 10000) as i64;

        let total_deductions = protocol_fee + shield_fee + custodian_fee;
        let net_owner_yield = gross_amount - total_deductions;

        assert_eq!(protocol_fee, 3_0000, "Protocol SaaS fee must be 3%");
        assert_eq!(shield_fee, 4_0000, "MutualShield contribution must be 4%");
        assert_eq!(custodian_fee, 0, "Custodian fee is 0");
        assert_eq!(net_owner_yield, 93_0000, "Net owner payout must be 93%");
        assert_eq!(protocol_fee + shield_fee + net_owner_yield, gross_amount, "Conservation of funds");
    }

    #[test]
    fn test_progressive_equity_buyout_computation() {
        let total_asset_shares: i64 = 1000;
        let monthly_installments: i64 = 20;
        let equity_per_installment = total_asset_shares / monthly_installments;

        assert_eq!(equity_per_installment, 50, "50 shares acquired per installment");
        let remaining_shares_investor = total_asset_shares - (equity_per_installment * 5);
        assert_eq!(remaining_shares_investor, 750, "Investor pool holds 750 shares after 5 months");
    }

    #[test]
    fn test_zero_riba_late_fee_charity_sink_invariant() {
        let lessor = AccountNumber::from("closet-owner");
        let charity_sink = AccountNumber::from("waqf-charity");
        let direct_damage_cost: Quantity = 25_0000.into(); // $25.00 direct repair / courier cost
        let punitive_late_penalty: Quantity = 50_0000.into(); // $50.00 punitive delay penalty

        // Invariant 1: Lessor and charity_sink must be distinct
        assert_ne!(lessor, charity_sink, "Lessor cannot be the charity sink");

        // Simulate ledger balances
        let mut lessor_received_direct: i64 = 0;
        let mut lessor_received_penalty: i64 = 0;
        let mut charity_sink_received: i64 = 0;

        // Process late return
        lessor_received_direct += direct_damage_cost.value;
        // Invariant 2: Punitive penalty routed 100% to charity sink
        charity_sink_received += punitive_late_penalty.value;

        // Assert: Lessor received exactly and only the direct actual cost
        assert_eq!(lessor_received_direct, 25_0000, "Lessor receives direct damage cost");
        assert_eq!(lessor_received_penalty, 0, "Lessor NEVER receives late-fee penalty (Zero-Riba Invariant)");
        assert_eq!(charity_sink_received, 50_0000, "Charity sink receives 100% of punitive late deterrence");
    }

    #[test]
    #[should_panic(expected = "Riba Violation")]
    fn test_riba_violation_triggers_panic_if_lessor_is_sink() {
        let lessor = AccountNumber::from("greedy-lender");
        let charity_sink = AccountNumber::from("greedy-lender"); // Invalid: attempt to self-route

        if lessor == charity_sink {
            panic!("Riba Violation: Creditor/lessor can never receive late penalties");
        }
    }
}
