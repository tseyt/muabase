# Choose Your Institutional App

Muabase provides three composable, verifiable institutional application blueprints built on shared primitives (`rust/muabase_institutional`, `libraries/muabase_db` `dwal_merkle` proofs, selective privacy, and `compliance.md` deterministic hooks):

| Application | Primary Users | Core Services | Key Guarantees | Quickstart |
|---|---|---|---|---|
| **App 1: Verifiable Humanitarian & Zakat Corridor** | Donors, Diaspora Relief, Licensed Bridge Desks, Field Verifiers | `credential`, `staged-tx`, `cred-clear`, `ext-vault` | 100% *Tamlik* zero-fee principal (`ZAKAT_TAMLIK_100`), zero-PII salted commitments + selective disclosure, `>= 2` accredited witness quorum, offline `dwal_merkle` receipt proof | [Humanitarian Corridor Quickstart](humanitarian-corridor.md) |
| **App 2: Programmable *Waqf* & Masjid Treasury OS** | Endowment Founders, Masjid Boards, Treasurers, Congregants | `waqf-treas`, `auth-delg`, `auth-dyn`, `auth-rcovr`, `staged-tx` | Immutable *Hifz Asl al-Waqf* corpus lock, *shurut* category + cap enforcement, 2-of-3 board-quorum disbursements, public aggregate conservation, board-quorum treasurer key recovery | [Waqf & Masjid Treasury Quickstart](waqf-masjid-treasury.md) |
| **App 3: Commercial *Muamalat* + Native *Tahkim* Arbitration** | SMEs, Suppliers, Investors, Shariah Scholars, Arbitrators | `muamalat`, `staged-tx`, `credential` | Scholar-attested *Istisna'* / *Mudarabah* template pinning, milestone escrow, zero-plaintext B2B terms, dispute freeze, 2-of-3 *Tahkim* binding award with audit receipt | [Muamalat & Tahkim Quickstart](muamalat-tahkim.md) |
