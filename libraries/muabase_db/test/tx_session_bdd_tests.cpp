#include "../../triedent/test/bdd_lite.hpp"

#include <muabase_db/tx_session.hpp>

#include <cstdio>
#include <filesystem>
#include <string>
#include <string_view>
#include <unistd.h>
#include <vector>

using namespace muabase_db;

namespace
{
   struct ScopedDir
   {
      std::filesystem::path path;
      explicit ScopedDir(std::string_view tag)
      {
         static std::uint64_t seq = 0;
         path = std::filesystem::temp_directory_path() /
                (std::string(tag) + "_" + std::to_string(::getpid()) + "_" + std::to_string(++seq));
         std::filesystem::remove_all(path);
         std::filesystem::create_directories(path);
      }
      ~ScopedDir()
      {
         std::error_code ec;
         std::filesystem::remove_all(path, ec);
      }
   };

   std::span<const std::byte> bspan(std::string_view sv)
   {
      return {reinterpret_cast<const std::byte*>(sv.data()), sv.size()};
   }

   std::string sview(std::span<const std::byte> sp)
   {
      return {reinterpret_cast<const char*>(sp.data()), sp.size()};
   }
}  // namespace

SCENARIO("TxSession tracks multi-service subtrees across nested savepoint frames without detached loss")
{
   GIVEN("an initial world state with LeaseEngine and MutualShield service subtrees")
   {
      ScopedDir        dir{"tx_subtree_frames"};
      SegmentAllocator sal{dir.path, SalConfig{.segment_bytes = 64 * 1024}};
      CowArt           art{sal};

      ArtRoot base_world;
      {
         TxSession init{art, base_world};
         auto      leases = init.open_subtree(bspan("svc/lease-engine"));
         auto      shield = init.open_subtree(bspan("svc/mutual-shield"));
         leases.upsert(bspan("lease/001"), bspan("active_cnc"));
         shield.upsert(bspan("pool/reserve"), bspan("10000_MUA"));
         base_world = init.commit().post_root;
      }

      WHEN("outer and nested frames mutate multiple subtrees, one nested frame aborts, and another commits")
      {
         TxSession tx{art, base_world};
         auto      leases = tx.open_subtree(bspan("svc/lease-engine"));
         auto      shield = tx.open_subtree(bspan("svc/mutual-shield"));

         // Outer frame mutation
         leases.upsert(bspan("lease/002"), bspan("active_forklift"));

         // Nested frame #1: performs invalid payout -> aborted!
         tx.push_frame();
         REQUIRE(tx.frame_depth() == 1);
         shield.upsert(bspan("pool/reserve"), bspan("0_MUA_DRAINED"));
         leases.upsert(bspan("lease/999_aborted"), bspan("ghost_lease"));
         tx.abort_frame();
         REQUIRE(tx.frame_depth() == 0);

         // Nested frame #2: valid ClaimCouncil payout -> committed!
         tx.push_frame();
         shield.upsert(bspan("pool/reserve"), bspan("9500_MUA"));
         auto council = tx.open_subtree(bspan("svc/claim-council"));
         council.upsert(bspan("claim/101"), bspan("paid_500_MUA"));
         tx.commit_frame();
         REQUIRE(tx.frame_depth() == 0);

         // Re-opening subtree before commit sees live uncommitted frame state
         auto shield_reopened = tx.open_subtree(bspan("svc/mutual-shield"));
         auto live_reserve    = shield_reopened.get(bspan("pool/reserve"));
         REQUIRE(live_reserve.has_value() && sview(*live_reserve) == "9500_MUA");

         CommittedTx committed = tx.commit();

         THEN("committed world atomically persists outer + committed inner subtrees and drops aborted edits")
         {
            auto post_leases  = art.get_subtree(committed.post_root, bspan("svc/lease-engine"));
            auto post_shield  = art.get_subtree(committed.post_root, bspan("svc/mutual-shield"));
            auto post_council = art.get_subtree(committed.post_root, bspan("svc/claim-council"));
            REQUIRE(post_leases.has_value());
            REQUIRE(post_shield.has_value());
            REQUIRE(post_council.has_value());

            auto l1 = art.get(*post_leases, bspan("lease/001"));
            auto l2 = art.get(*post_leases, bspan("lease/002"));
            auto lg = art.get(*post_leases, bspan("lease/999_aborted"));
            REQUIRE(l1.has_value() && sview(*l1) == "active_cnc");
            REQUIRE(l2.has_value() && sview(*l2) == "active_forklift");
            REQUIRE(!lg.has_value());

            auto rsv = art.get(*post_shield, bspan("pool/reserve"));
            auto clm = art.get(*post_council, bspan("claim/101"));
            REQUIRE(rsv.has_value() && sview(*rsv) == "9500_MUA");
            REQUIRE(clm.has_value() && sview(*clm) == "paid_500_MUA");

            // Base world snapshot remains untouched
            auto base_shield = art.get_subtree(base_world, bspan("svc/mutual-shield"));
            REQUIRE(base_shield.has_value());
            auto base_rsv = art.get(*base_shield, bspan("pool/reserve"));
            REQUIRE(base_rsv.has_value() && sview(*base_rsv) == "10000_MUA");
         }
      }
   }
}

SCENARIO("Commutative 3-way merge yields identical state and Merkle roots regardless of merge order")
{
   GIVEN("two parallel agent transactions sharing a read-only config key and writing disjoint subtree keys")
   {
      ScopedDir        dir{"tx_commutative_merge"};
      SegmentAllocator sal{dir.path, SalConfig{.segment_bytes = 64 * 1024}};
      CowArt           art{sal};
      Merkleizer       merkle{art};

      ArtRoot base;
      {
         TxSession setup{art, base};
         setup.upsert(bspan("cfg/guild_fee_bps"), bspan("25"));
         auto leases = setup.open_subtree(bspan("svc/lease-engine"));
         for (int i = 0; i < 20; ++i)
         {
            leases.upsert(bspan("lease/seed_" + std::to_string(i)), bspan("seed"));
         }
         leases.upsert(bspan("lease/stale_a"), bspan("to_delete_a"));
         leases.upsert(bspan("lease/stale_b"), bspan("to_delete_b"));
         base = setup.commit().post_root;
      }

      // Parallel Transaction A: reads shared cfg, mutates disjoint keys in svc/lease-engine + svc/staged-tx
      TxSession s_a{art, base};
      REQUIRE(s_a.get(bspan("cfg/guild_fee_bps")).has_value());
      {
         auto leases = s_a.open_subtree(bspan("svc/lease-engine"));
         leases.upsert(bspan("lease/agent_A_1"), bspan("proposal_A1"));
         leases.upsert(bspan("lease/agent_A_2"), bspan("proposal_A2"));
         REQUIRE(leases.remove(bspan("lease/stale_a")));
         auto staged = s_a.open_subtree(bspan("svc/staged-tx"));
         staged.upsert(bspan("tx/1001"), bspan("sanad_A"));
      }
      CommittedTx tx_a = s_a.commit();

      // Parallel Transaction B: ALSO reads shared cfg, mutates disjoint keys in svc/lease-engine + svc/evals
      TxSession s_b{art, base};
      REQUIRE(s_b.get(bspan("cfg/guild_fee_bps")).has_value());
      {
         auto leases = s_b.open_subtree(bspan("svc/lease-engine"));
         leases.upsert(bspan("lease/agent_B_1"), bspan("proposal_B1"));
         REQUIRE(leases.remove(bspan("lease/stale_b")));
         auto evals = s_b.open_subtree(bspan("svc/evaluations"));
         evals.upsert(bspan("halaqah/42"), bspan("condorcet_winner"));
      }
      CommittedTx tx_b = s_b.commit();

      WHEN("tx_a and tx_b are merged in both orders: merge(base, A, B) vs merge(base, B, A)")
      {
         MergeResult ab = merge_transactions(art, base, tx_a, tx_b, &merkle);
         MergeResult ba = merge_transactions(art, base, tx_b, tx_a, &merkle);

         THEN("both merges succeed without conflict and produce bit-for-bit identical Merkle roots")
         {
            REQUIRE(ab.ok);
            REQUIRE(ba.ok);
            REQUIRE(!ab.conflict.has_value());
            REQUIRE(!ba.conflict.has_value());
            REQUIRE(ab.merkle_root != Checksum256{});
            REQUIRE(ab.merkle_root == ba.merkle_root);
            REQUIRE(ab.merged.changes == ba.merged.changes);

            // Verify both A's and B's edits inside the shared svc/lease-engine subtree survived!
            auto leases_ab = art.get_subtree(ab.merged.post_root, bspan("svc/lease-engine"));
            REQUIRE(leases_ab.has_value());
            REQUIRE(art.get(*leases_ab, bspan("lease/agent_A_1")).has_value());
            REQUIRE(art.get(*leases_ab, bspan("lease/agent_B_1")).has_value());
            REQUIRE(!art.get(*leases_ab, bspan("lease/stale_a")).has_value());
            REQUIRE(!art.get(*leases_ab, bspan("lease/stale_b")).has_value());

            // Verify top-level subtree digests ALSO match between ab and ba
            auto leases_ba = art.get_subtree(ba.merged.post_root, bspan("svc/lease-engine"));
            REQUIRE(leases_ba.has_value());
            REQUIRE(merkle.compute_root(*leases_ab) == merkle.compute_root(*leases_ba));
         }
      }
   }
}

SCENARIO("3-way merge isolates exact Read-Write and Write-Write conflict loci (Tahrir Mahall al-Niza')")
{
   GIVEN("a base snapshot queried via range cursors and mutated concurrently")
   {
      ScopedDir        dir{"tx_conflict_locus"};
      SegmentAllocator sal{dir.path, SalConfig{.segment_bytes = 64 * 1024}};
      CowArt           art{sal};

      ArtRoot base;
      {
         TxSession init{art, base};
         init.upsert(bspan("order/10"), bspan("100"));
         init.upsert(bspan("order/30"), bspan("300"));
         base = init.commit().post_root;
      }

      WHEN("Tx A performs a lower_bound scan over [order/15, order/30] and Tx B inserts phantom key order/20")
      {
         TxSession s_scan{art, base};
         auto      ge = s_scan.lower_bound(bspan("order/15"), 6);
         REQUIRE(ge.has_value() && sview(ge->key) == "order/30");
         CommittedTx tx_scan = s_scan.commit();

         TxSession s_phantom{art, base};
         s_phantom.upsert(bspan("order/20"), bspan("200_phantom"));
         CommittedTx tx_phantom = s_phantom.commit();

         MergeResult r1 = merge_transactions(art, base, tx_scan, tx_phantom);
         MergeResult r2 = merge_transactions(art, base, tx_phantom, tx_scan);

         // Also test symmetric Write-Write conflict on identical subtree key
         TxSession w1{art, base};
         w1.open_subtree(bspan("svc/tokens")).upsert(bspan("bal/alice"), bspan("90"));
         CommittedTx tx_w1 = w1.commit();

         TxSession w2{art, base};
         w2.open_subtree(bspan("svc/tokens")).upsert(bspan("bal/alice"), bspan("80"));
         CommittedTx tx_w2 = w2.commit();

         MergeResult ww1 = merge_transactions(art, base, tx_w1, tx_w2);
         MergeResult ww2 = merge_transactions(art, base, tx_w2, tx_w1);

         THEN("phantom Read-Write and concurrent Write-Write conflicts are caught symmetrically with exact loci")
         {
            REQUIRE(!r1.ok);
            REQUIRE(!r2.ok);
            REQUIRE(r1.conflict.has_value());
            REQUIRE(r1.conflict == r2.conflict);
            REQUIRE(r1.conflict->kind == ConflictKind::ReadWrite);
            REQUIRE(!r1.conflict->locus_lower.empty());

            REQUIRE(!ww1.ok);
            REQUIRE(!ww2.ok);
            REQUIRE(ww1.conflict.has_value());
            REQUIRE(ww1.conflict == ww2.conflict);
            REQUIRE(ww1.conflict->kind == ConflictKind::WriteWrite);
         }
      }
   }
}

int main()
{
   return bdd_lite::run_all("MuabaseDB Transactional Subtrees + Commutative 3-Way Merge BDD Suite");
}
