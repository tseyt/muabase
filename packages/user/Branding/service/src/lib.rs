#[psibase::service_tables]
mod tables {
    use async_graphql::SimpleObject;
    use psibase::{Pack, ToSchema, Unpack};
    use serde::{Deserialize, Serialize};

    #[table(name = "NetworkNameTable")]
    #[derive(Pack, Unpack, ToSchema, SimpleObject, Serialize, Deserialize, Debug)]
    pub struct NetworkName {
        pub name: String,
    }

    impl NetworkName {
        #[primary_key]
        fn by_key(&self) {}
    }

    impl Default for NetworkName {
        fn default() -> Self {
            NetworkName {
                name: String::from("psibase"),
            }
        }
    }

    #[table(name = "LocaleTable")]
    #[derive(Pack, Unpack, ToSchema, SimpleObject, Serialize, Deserialize, Debug, Clone)]
    pub struct TerminologyLocale {
        pub locale: String,
    }

    impl TerminologyLocale {
        #[primary_key]
        fn by_key(&self) {}
    }

    impl Default for TerminologyLocale {
        fn default() -> Self {
            TerminologyLocale {
                locale: String::from("institutional"),
            }
        }
    }

    #[derive(Pack, Unpack, ToSchema, SimpleObject, Serialize, Deserialize, Debug, Clone)]
    pub struct TerminologyMapping {
        pub key: String,
        pub institutional: String,
        pub muamalat: String,
    }

    pub fn get_canonical_terminology_mappings() -> Vec<TerminologyMapping> {
        vec![
            TerminologyMapping {
                key: "asset_pool".to_string(),
                institutional: "RWA Asset Series / Series LLC".to_string(),
                muamalat: "Sukuk al-Qirad / Musharakah".to_string(),
            },
            TerminologyMapping {
                key: "lease".to_string(),
                institutional: "Fixed Usufruct Lease / P2P Rental".to_string(),
                muamalat: "Ijarah al-A‘yan wa al-Manafi‘".to_string(),
            },
            TerminologyMapping {
                key: "lease_to_own".to_string(),
                institutional: "Progressive Lease-to-Own".to_string(),
                muamalat: "Musharakah Mutanaqisah".to_string(),
            },
            TerminologyMapping {
                key: "mutual_shield".to_string(),
                institutional: "Mutual Protection Reserve Pool".to_string(),
                muamalat: "Takaful (Tabarru‘ & Ta‘awun)".to_string(),
            },
            TerminologyMapping {
                key: "claim_council".to_string(),
                institutional: "Peer Triage Committee".to_string(),
                muamalat: "Majlis al-Tahkim & Shura".to_string(),
            },
            TerminologyMapping {
                key: "evals".to_string(),
                institutional: "Objective Bounty & Claim Ranking".to_string(),
                muamalat: "Ju‘alah Halaqa".to_string(),
            },
            TerminologyMapping {
                key: "endowment_policy".to_string(),
                institutional: "Trust Principal Preservation Lock".to_string(),
                muamalat: "Hifz Asl al-Waqf".to_string(),
            },
            TerminologyMapping {
                key: "priority_window".to_string(),
                institutional: "Co-Owner Right of First Refusal".to_string(),
                muamalat: "Haqq al-Shuf‘ah".to_string(),
            },
            TerminologyMapping {
                key: "charity_sink".to_string(),
                institutional: "Public Endowment Charity Sink".to_string(),
                muamalat: "Masraf al-Sadaqah".to_string(),
            },
            TerminologyMapping {
                key: "credit_clear".to_string(),
                institutional: "Multilateral Invoice Netting".to_string(),
                muamalat: "Muqassah & Wajahah-Hawalah".to_string(),
            },
            TerminologyMapping {
                key: "trust_score".to_string(),
                institutional: "Reliability & Audit Score".to_string(),
                muamalat: "‘Adalah (Probity / Reliability)".to_string(),
            },
            TerminologyMapping {
                key: "domain_badge".to_string(),
                institutional: "Domain Competence Credential".to_string(),
                muamalat: "Ikhtisas (Domain Specialization)".to_string(),
            },
            TerminologyMapping {
                key: "tangible_ratio".to_string(),
                institutional: "Tangible Asset Backing Ratio (>= 51%)".to_string(),
                muamalat: "Nisbat al-‘Ayn al-Haqiqiyyah".to_string(),
            },
        ]
    }
}

#[psibase::service(name = "branding", tables = "tables")]
mod service {
    use crate::tables::*;
    use async_graphql::Object;
    use psibase::services::auth_delegate::Wrapper as AuthDelegate;
    use psibase::services::http_server::Wrapper as HttpServer;
    use psibase::services::sites::Wrapper as Sites;
    use psibase::*;

    /// Sets the network name for the current network.
    #[action]
    #[allow(non_snake_case)]
    fn setNetworkName(name: String) {
        assert_eq!(
            get_sender(),
            SERVICE,
            "Only the 'branding' account is authorized to set network branding",
        );

        let account = AccountNumber::from_exact(name.as_str()).expect("Network name invalid");

        let created = AuthDelegate::call().newAccount(
            account,
            SERVICE,
            services::accounts::NewAccountMode::MATCH_EXISTING,
        );
        if !created {
            HttpServer::call_as(account).clearRedirect();
        }
        Sites::call_as(account).setProxy(account!("homepage"));

        let table = NetworkNameTable::new();
        if let Some(prev) = table.get_index_pk().get(&()) {
            let prev_account = AccountNumber::from_exact(prev.name.as_str()).unwrap();
            if prev_account != account {
                HttpServer::call_as(prev_account).setRedirect(account);
            }
        }

        table.put(&NetworkName { name: name.clone() }).unwrap();

        Wrapper::emit().history().networkNameChanged(name);
    }

    /// Sets the terminology locale: "institutional" or "muamalat".
    #[action]
    #[allow(non_snake_case)]
    fn setTerminologyLocale(locale: String) {
        assert!(
            locale == "institutional" || locale == "muamalat",
            "Locale must be either 'institutional' or 'muamalat'"
        );

        let table = LocaleTable::new();
        table.put(&TerminologyLocale { locale: locale.clone() }).unwrap();
        Wrapper::emit().history().terminologyLocaleChanged(locale);
    }

    #[event(history)]
    pub fn networkNameChanged(name: String) {}

    #[event(history)]
    pub fn terminologyLocaleChanged(locale: String) {}

    struct Query;

    #[Object]
    impl Query {
        async fn network_name(&self) -> String {
            let curr_val = NetworkNameTable::read().get_index_pk().get(&());
            curr_val.unwrap_or_default().name
        }

        async fn terminology_locale(&self) -> String {
            let curr = LocaleTable::read().get_index_pk().get(&());
            curr.unwrap_or_default().locale
        }

        async fn terminology_dictionary(&self) -> Vec<TerminologyMapping> {
            get_canonical_terminology_mappings()
        }

        async fn translate_term(&self, term: String) -> String {
            let lookup_term = match term.as_str() {
                "rwa_series" | "rwa" => "asset_pool",
                "usufruct_lease_fixed" | "usufruct" | "rental" => "lease",
                "usufruct_lease_progressive" | "rent_to_own" => "lease_to_own",
                "evaluations" => "evals",
                "rofr_window" | "rofr" => "priority_window",
                "adalah_score" => "trust_score",
                "ikhtisas_tag" => "domain_badge",
                _ => term.as_str(),
            };
            let current_locale = LocaleTable::read().get_index_pk().get(&()).unwrap_or_default().locale;
            for mapping in get_canonical_terminology_mappings() {
                if mapping.key == lookup_term {
                    return if current_locale == "muamalat" {
                        mapping.muamalat
                    } else {
                        mapping.institutional
                    };
                }
            }
            term
        }
    }

    #[action]
    #[allow(non_snake_case)]
    fn serveSys(
        request: HttpRequest,
        _socket: Option<i32>,
        _user: Option<AccountNumber>,
    ) -> Option<HttpReply> {
        None.or_else(|| serve_graphql(&request, Query))
    }
}
