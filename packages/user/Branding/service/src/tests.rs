#[cfg(all(test, target_family = "wasm"))]
mod tests {
    #[psibase::test_case(packages("Branding"))]
    fn package_installs(_chain: psibase::Chain) -> Result<(), psibase::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod unit_tests {
    use crate::tables::{resolve_term_for_locale, validate_locale_update, NetworkName};

    #[test]
    fn test_default_network_name_is_muabase() {
        assert_eq!(NetworkName::default().name, "muabase");
    }

    #[test]
    fn test_locale_update_requires_service_auth_and_valid_locale() {
        assert!(validate_locale_update(true, "institutional").is_ok());
        assert!(validate_locale_update(true, "muamalat").is_ok());
        assert!(validate_locale_update(false, "muamalat").is_err());
        assert!(validate_locale_update(true, "unknown").is_err());
    }

    #[test]
    fn test_muamalat_and_institutional_humanitarian_corridor_terminology() {
        assert_eq!(
            resolve_term_for_locale("zakat_tamlik", "muamalat"),
            "Zakat Tamlik Vault (Tamlik al-Asnaf 100%)"
        );
        assert_eq!(
            resolve_term_for_locale("asnaf_gate", "muamalat"),
            "Asnaf Verification Gate (Bayyinat Istihqaq al-Zakat)"
        );
        assert_eq!(
            resolve_term_for_locale("bridge_liquidity_desk", "muamalat"),
            "Bridge Liquidity Desk (Maktab al-Sarf wa al-Hawalah)"
        );
        assert_eq!(
            resolve_term_for_locale("zakat_tamlik_vault", "institutional"),
            "Zero-Fee Direct Principal Vault (100% Beneficiary Delivery)"
        );
        assert_eq!(
            resolve_term_for_locale("waqf_treasury", "muamalat"),
            "Waqf & Masjid Treasury OS (Hifz Asl al-Waqf)"
        );
        assert_eq!(
            resolve_term_for_locale("muamalat_tahkim", "institutional"),
            "Commercial Milestone Escrow & 2-of-3 Arbitration"
        );
    }
}
