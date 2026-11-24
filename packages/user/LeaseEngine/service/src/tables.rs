#[psibase::service_tables]
pub mod tables {
    use async_graphql::SimpleObject;
    use psibase::services::tokens::{Quantity, TID};
    use psibase::{AccountNumber, Checksum256, Pack, Table, TimePointSec, ToSchema, Unpack};
    use serde::{Deserialize, Serialize};

    pub const STATUS_CREATED: u8 = 0;
    pub const STATUS_PRE_CONDITION_SIGNED: u8 = 1;
    pub const STATUS_ACTIVE: u8 = 2;
    pub const STATUS_RETURNED_CLEAN: u8 = 3;
    pub const STATUS_DISPUTED: u8 = 4;
    pub const STATUS_COMPLETED: u8 = 5;

    #[table(name = "InitTable", index = 0)]
    #[derive(Serialize, Deserialize, ToSchema, Pack, Unpack, Debug, Default)]
    pub struct InitRow {}
    impl InitRow {
        #[primary_key]
        fn pk(&self) {}
    }

    #[table(name = "LeaseIdTable", index = 1)]
    #[derive(Serialize, Deserialize, ToSchema, Pack, Unpack, Debug, Default)]
    pub struct LeaseIdRow {
        pub next_id: u64,
    }
    impl LeaseIdRow {
        #[primary_key]
        fn pk(&self) {}

        pub fn next_id() -> u64 {
            let table = LeaseIdTable::new();
            let mut row = table.get_index_pk().get(&()).unwrap_or_default();
            row.next_id += 1;
            table.put(&row).unwrap();
            row.next_id
        }
    }

    /// Usufruct Lease (Ijarah / P2P Rental / Diminishing Musharakah Buyout)
    #[table(name = "LeaseTable", index = 2)]
    #[derive(Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone)]
    pub struct UsufructLease {
        pub lease_id: u64,
        pub lessor: AccountNumber,
        pub lessee: AccountNumber,
        pub token_id: TID,
        pub nft_id: Option<u32>,
        pub duration_sec: u64,
        pub gross_payment: Quantity,
        pub payment_token_id: TID,
        pub start_time: Option<TimePointSec>,
        pub due_time: Option<TimePointSec>,
        pub returned_time: Option<TimePointSec>,
        pub pre_condition_evidence_hash: Checksum256,
        pub post_condition_evidence_hash: Option<Checksum256>,
        pub status: u8,
        pub is_progressive_buyout: bool,
        pub buyout_equity_units: Quantity,
        pub protocol_saas_fee_bps: u16,
        pub mutual_shield_fee_bps: u16,
        pub custodian_fee_bps: u16,
        pub custodian_account: AccountNumber,
        pub charity_sink_account: AccountNumber,
    }

    impl UsufructLease {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.lease_id
        }

        #[secondary_key(1)]
        fn by_lessee(&self) -> (AccountNumber, u64) {
            (self.lessee, self.lease_id)
        }

        #[secondary_key(2)]
        fn by_lessor(&self) -> (AccountNumber, u64) {
            (self.lessor, self.lease_id)
        }
    }
}
