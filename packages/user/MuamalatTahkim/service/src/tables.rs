#[psibase::service_tables]
pub mod tables {
    use async_graphql::SimpleObject;
    use psibase::{AccountNumber, Checksum256, Pack, Table, ToSchema, Unpack};
    use serde::{Deserialize, Serialize};

    #[table(name = "InitTable", index = 0)]
    #[derive(Serialize, Deserialize, ToSchema, Pack, Unpack, Debug, Default)]
    pub struct InitRow {}
    impl InitRow {
        #[primary_key]
        fn pk(&self) {}
    }

    #[table(name = "SeqIdTable", index = 1)]
    #[derive(Serialize, Deserialize, ToSchema, Pack, Unpack, Debug, Default)]
    pub struct SeqIdRow {
        pub next_template_id: u64,
        pub next_contract_id: u64,
    }
    impl SeqIdRow {
        #[primary_key]
        fn pk(&self) {}

        pub fn next_template() -> u64 {
            let table = SeqIdTable::new();
            let mut row = table.get_index_pk().get(&()).unwrap_or_default();
            row.next_template_id += 1;
            table.put(&row).unwrap();
            row.next_template_id
        }

        pub fn next_contract() -> u64 {
            let table = SeqIdTable::new();
            let mut row = table.get_index_pk().get(&()).unwrap_or_default();
            row.next_contract_id += 1;
            table.put(&row).unwrap();
            row.next_contract_id
        }
    }

    /// Scholar-attested commercial contract template (`ISTISNA` or `MUDARABAH`).
    #[table(name = "TemplateTable", index = 2)]
    #[derive(
        Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone, PartialEq, Eq,
    )]
    pub struct ContractTemplate {
        pub template_id: u64,
        pub contract_kind: String,
        pub scholar_account: AccountNumber,
        pub template_hash: Checksum256,
        pub certificate_hash: Checksum256,
        pub standard_ref: String,
        pub active: bool,
    }
    impl ContractTemplate {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.template_id
        }
    }

    /// Deterministic escrowed commercial Muamalat contract state machine.
    #[table(name = "ContractTable", index = 3)]
    #[derive(
        Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone, PartialEq, Eq,
    )]
    pub struct MuamalatContract {
        pub contract_id: u64,
        pub template_id: u64,
        pub template_hash: Checksum256,
        pub contract_kind: String,
        pub funder: AccountNumber,
        pub contractor: AccountNumber,
        pub arbitrators: Vec<AccountNumber>,
        pub total_escrow: u64,
        pub released_amount: u64,
        pub refunded_amount: u64,
        pub milestone_amounts: Vec<u64>,
        pub milestones_released: u32,
        pub terms_commitment_hash: Checksum256,
        pub status: String,
        pub last_receipt_leaf_hash: Option<Checksum256>,
    }
    impl MuamalatContract {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.contract_id
        }
    }

    /// Active or resolved 2-of-3 Tahkim arbitration dispute record.
    #[table(name = "DisputeTable", index = 4)]
    #[derive(
        Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone, PartialEq, Eq,
    )]
    pub struct TahkimDispute {
        pub contract_id: u64,
        pub raised_by: AccountNumber,
        pub evidence_commitment_hash: Checksum256,
        pub votes_json: Vec<String>,
        pub resolved: bool,
        pub final_ruling: Option<String>,
        pub award_receipt_leaf_hash: Option<Checksum256>,
    }
    impl TahkimDispute {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.contract_id
        }
    }
}
