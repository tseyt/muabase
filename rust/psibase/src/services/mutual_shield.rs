#[crate::service(name = "mut-shield", dispatch = false, psibase_mod = "crate")]
#[allow(non_snake_case, unused_variables)]
pub mod service {
    use crate::{services::tokens::Quantity, AccountNumber, Checksum256};

    #[action]
    fn init() {
        unimplemented!()
    }

    #[action]
    pub fn contribute(pool_id: u32, amount: Quantity, from: AccountNumber) {
        unimplemented!()
    }

    #[action]
    pub fn disbursePayout(
        claim_id: u64,
        pool_id: u32,
        recipient: AccountNumber,
        amount: Quantity,
        evidence_hash: Checksum256,
    ) {
        unimplemented!()
    }

    #[action]
    pub fn distributeSurplus(pool_id: u32, epoch_id: u32) {
        unimplemented!()
    }
}
