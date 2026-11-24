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
        pub next_endowment_id: u64,
        pub next_proposal_id: u64,
        pub next_recovery_id: u64,
    }
    impl SeqIdRow {
        #[primary_key]
        fn pk(&self) {}

        pub fn next_endowment() -> u64 {
            let table = SeqIdTable::new();
            let mut row = table.get_index_pk().get(&()).unwrap_or_default();
            row.next_endowment_id += 1;
            table.put(&row).unwrap();
            row.next_endowment_id
        }

        pub fn next_proposal() -> u64 {
            let table = SeqIdTable::new();
            let mut row = table.get_index_pk().get(&()).unwrap_or_default();
            row.next_proposal_id += 1;
            table.put(&row).unwrap();
            row.next_proposal_id
        }

        pub fn next_recovery() -> u64 {
            let table = SeqIdTable::new();
            let mut row = table.get_index_pk().get(&()).unwrap_or_default();
            row.next_recovery_id += 1;
            table.put(&row).unwrap();
            row.next_recovery_id
        }
    }

    /// Rule-locked Waqf / Masjid Endowment with immutable corpus preservation (`Hifz Asl al-Waqf`).
    #[table(name = "EndowmentTable", index = 2)]
    #[derive(
        Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone, PartialEq, Eq,
    )]
    pub struct Endowment {
        pub endowment_id: u64,
        pub founder: AccountNumber,
        pub label: String,
        pub corpus_principal: u64,
        pub yield_balance: u64,
        pub total_disbursed: u64,
        pub shurut_hash: Checksum256,
        pub scholar_attestation_hash: Checksum256,
        pub allowed_categories: Vec<String>,
        pub max_single_disbursement: u64,
        pub board_members: Vec<AccountNumber>,
        pub quorum_threshold: u32,
        pub principal_locked: bool,
    }
    impl Endowment {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.endowment_id
        }
    }

    /// Board-quorum disbursement proposal & settled Merkle audit receipt.
    #[table(name = "DisbursementTable", index = 3)]
    #[derive(
        Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone, PartialEq, Eq,
    )]
    pub struct DisbursementProposal {
        pub proposal_id: u64,
        pub endowment_id: u64,
        pub proposer: AccountNumber,
        pub recipient: AccountNumber,
        pub category: String,
        pub amount: u64,
        pub evidence_commitment_hash: Checksum256,
        pub approvals: Vec<AccountNumber>,
        pub executed: bool,
        pub receipt_leaf_hash: Option<Checksum256>,
    }
    impl DisbursementProposal {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.proposal_id
        }
    }

    /// Board-quorum treasurer key rotation / recovery proposal.
    #[table(name = "KeyRecoveryTable", index = 4)]
    #[derive(
        Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone, PartialEq, Eq,
    )]
    pub struct KeyRecoveryProposal {
        pub recovery_id: u64,
        pub endowment_id: u64,
        pub old_member: AccountNumber,
        pub new_member: AccountNumber,
        pub new_pubkey_spki_hash: Checksum256,
        pub approvals: Vec<AccountNumber>,
        pub executed: bool,
    }
    impl KeyRecoveryProposal {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.recovery_id
        }
    }

    #[derive(Serialize, Deserialize, SimpleObject, Debug, Clone, PartialEq, Eq, Default)]
    pub struct AggregateTreasury {
        pub endowment_count: u64,
        pub total_corpus_locked: u64,
        pub total_yield_available: u64,
        pub total_disbursed: u64,
    }
}
