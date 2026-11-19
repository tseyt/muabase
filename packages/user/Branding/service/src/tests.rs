#[cfg(all(test, target_family = "wasm"))]
mod tests {
    #[psibase::test_case(packages("Branding"))]
    fn package_installs(_chain: psibase::Chain) -> Result<(), psibase::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod unit_tests {
    use crate::tables::resolve_term_for_locale;

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
    }
}
