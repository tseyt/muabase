#include "../../triedent/test/bdd_lite.hpp"

#include <muabase_db/dwal_merkle.hpp>

#include <cstdio>
#include <filesystem>
#include <fstream>
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

   std::string pad_key(int n)
   {
      char buf[32];
      std::snprintf(buf, sizeof(buf), "claim/%05d", n);
      return buf;
   }
}  // namespace

SCENARIO("DWAL buffered writes serve live merged reads across RW, frozen RO, and persistent COW-ART")
{
   GIVEN("a persistent CowArt base snapshot on root 0 and an independent shard on root 511")
   {
      ScopedDir        dir{"dwal_live_merge"};
      SegmentAllocator sal{dir.path / "sal", SalConfig{.segment_bytes = 64 * 1024}};
      CowArt           art{sal};

      ArtRoot base0;
      base0 = art.upsert(base0, bspan("k/10"), bspan("base_10"));
      base0 = art.upsert(base0, bspan("k/20"), bspan("base_20"));
      base0 = art.upsert(base0, bspan("k/30"), bspan("base_30"));
      base0 = art.upsert(base0, bspan("k/40"), bspan("base_40"));

      DwalEngine dwal{dir.path / "wal", art};
      dwal.set_base_root(0, base0);

      WHEN("writes and tombstones span frozen RO and active RW before batch flush")
      {
         // Layer 1 (will become frozen RO): overwrite k/20, delete k/30, insert k/25
         dwal.upsert(0, bspan("k/20"), bspan("ro_20"));
         dwal.remove(0, bspan("k/30"));
         dwal.upsert(0, bspan("k/25"), bspan("ro_25"));
         REQUIRE(dwal.swap_rw_to_ro(0));

         // Layer 2 (active RW): delete k/25, overwrite k/10, delete k/40, insert k/35
         dwal.remove(0, bspan("k/25"));
         dwal.upsert(0, bspan("k/10"), bspan("rw_10"));
         dwal.remove(0, bspan("k/40"));
         dwal.upsert(0, bspan("k/35"), bspan("rw_35"));

         // Independent root 511 mutation does not pollute root 0
         dwal.upsert(511, bspan("k/15"), bspan("shard_511"));

         THEN("live point and cursor reads reflect exact RW -> RO -> COW-ART precedence")
         {
            // Live keys on root 0 should be exactly: k/10=rw_10, k/20=ro_20, k/35=rw_35
            auto v10 = dwal.get(0, bspan("k/10"));
            auto v20 = dwal.get(0, bspan("k/20"));
            auto v25 = dwal.get(0, bspan("k/25"));
            auto v30 = dwal.get(0, bspan("k/30"));
            auto v35 = dwal.get(0, bspan("k/35"));
            auto v40 = dwal.get(0, bspan("k/40"));
            REQUIRE(v10.has_value() && sview(*v10) == "rw_10");
            REQUIRE(v20.has_value() && sview(*v20) == "ro_20");
            REQUIRE(!v25.has_value());
            REQUIRE(!v30.has_value());
            REQUIRE(v35.has_value() && sview(*v35) == "rw_35");
            REQUIRE(!v40.has_value());

            auto lb_after_20 = dwal.lower_bound(0, bspan("k/21"), 2);
            REQUIRE(lb_after_20.has_value() && sview(lb_after_20->key) == "k/35");

            auto lt_before_35 = dwal.less_than(0, bspan("k/35"), 2);
            REQUIRE(lt_before_35.has_value() && sview(lt_before_35->key) == "k/20");

            auto mx = dwal.max_with_prefix(0, bspan("k/"));
            REQUIRE(mx.has_value() && sview(mx->key) == "k/35");

            auto v511 = dwal.get(511, bspan("k/15"));
            REQUIRE(v511.has_value() && sview(*v511) == "shard_511");
            REQUIRE(!dwal.get(0, bspan("k/15")).has_value());
         }

         AND_THEN("flushing drains RO and RW into a persistent CowArt snapshot")
         {
            ArtRoot flushed = dwal.flush(0);
            REQUIRE(art.count(flushed) == 3);
            REQUIRE(dwal.root_stats(0).rw_entries == 0);
            REQUIRE(dwal.root_stats(0).ro_entries == 0);

            auto p10 = art.get(flushed, bspan("k/10"));
            auto p20 = art.get(flushed, bspan("k/20"));
            auto p35 = art.get(flushed, bspan("k/35"));
            REQUIRE(p10.has_value() && sview(*p10) == "rw_10");
            REQUIRE(p20.has_value() && sview(*p20) == "ro_20");
            REQUIRE(p35.has_value() && sview(*p35) == "rw_35");
            REQUIRE(!art.get(flushed, bspan("k/25")).has_value());
            REQUIRE(!art.get(flushed, bspan("k/30")).has_value());
         }
      }
   }
}

SCENARIO("DWAL CRC32-framed WAL recovers unflushed writes across crash and ignores torn tail bytes")
{
   GIVEN("a DWAL directory with one flushed epoch, subsequent unflushed writes, and a torn tail frame")
   {
      ScopedDir        dir{"dwal_crash_recovery"};
      SegmentAllocator sal{dir.path / "sal", SalConfig{.segment_bytes = 64 * 1024}};
      CowArt           art{sal};
      ArtRoot          persisted_epoch1;

      std::filesystem::path wal_file;
      {
         DwalEngine dwal{dir.path / "wal", art, DwalConfig{.sync_wal_on_write = true}};
         dwal.upsert(2, bspan("epoch1/a"), bspan("flushed_a"));
         persisted_epoch1 = dwal.flush(2);

         // Unflushed buffered mutations committed to WAL before simulated crash:
         dwal.upsert(2, bspan("epoch2/b"), bspan("recovered_b"));
         dwal.upsert(2, bspan("epoch2/c"), bspan("temp_c"));
         dwal.remove(2, bspan("epoch2/c"));
         dwal.remove(2, bspan("epoch1/a"));
         wal_file = dwal.wal_file_path();
      }

      // Simulate power-loss torn write at the end of dwal.log
      {
         std::ofstream out(wal_file, std::ios::binary | std::ios::app);
         const char    torn_bytes[] = "\x4D\x57\x41\x4C\x02\x00\x01\x99\x88";
         out.write(torn_bytes, sizeof(torn_bytes) - 1);
      }

      WHEN("a fresh DwalEngine reopens the WAL directory atop persisted_epoch1")
      {
         DwalEngine reopened{dir.path / "wal", art};
         reopened.set_base_root(2, persisted_epoch1);

         THEN("all valid post-checkpoint frames are replayed and the torn tail is safely discarded")
         {
            REQUIRE(reopened.root_stats(2).wal_replayed == 4);
            auto vb = reopened.get(2, bspan("epoch2/b"));
            REQUIRE(vb.has_value() && sview(*vb) == "recovered_b");
            REQUIRE(!reopened.get(2, bspan("epoch2/c")).has_value());
            REQUIRE(!reopened.get(2, bspan("epoch1/a")).has_value());
         }
      }
   }
}

SCENARIO("Canonical Patricia Merkleizer is history-independent and memoizes unmodified SAL subtrees")
{
   GIVEN("two CowArt trees populated with the same final 200 KV pairs in opposite orders + deletions")
   {
      ScopedDir        dir{"merkle_canonical"};
      SegmentAllocator sal{dir.path, SalConfig{.segment_bytes = 64 * 1024}};
      CowArt           art{sal};

      ArtRoot forward_root;
      for (int i = 0; i < 200; ++i)
      {
         forward_root = art.upsert(forward_root, bspan(pad_key(i)), bspan("val_" + std::to_string(i)));
      }

      // Build reverse_root in descending order AND insert+delete 40 extra keys to perturb physical leaves
      ArtRoot reverse_root;
      for (int i = 239; i >= 0; --i)
      {
         reverse_root = art.upsert(reverse_root, bspan(pad_key(i)), bspan("val_" + std::to_string(i)));
      }
      for (int i = 200; i < 240; ++i)
      {
         reverse_root = art.remove(reverse_root, bspan(pad_key(i))).first;
      }

      WHEN("Merkleizer computes state roots across both trees and across an incremental single-key block")
      {
         Merkleizer m_fwd{art};
         Merkleizer m_rev{art};

         Checksum256 root_fwd = m_fwd.compute_root(forward_root);
         Checksum256 root_rev = m_rev.compute_root(reverse_root);

         auto hashed_initial = m_fwd.stats().nodes_hashed;
         ArtRoot next_block  = art.upsert(forward_root, bspan(pad_key(7)), bspan("val_7_v2"));
         Checksum256 root_v2 = m_fwd.compute_root(next_block);
         auto hashed_delta   = m_fwd.stats().nodes_hashed - hashed_initial;

         THEN("canonical SHA-256 roots match bit-for-bit and incremental update hits memoized subtrees")
         {
            REQUIRE(root_fwd != Checksum256{});
            REQUIRE(root_fwd == root_rev);
            REQUIRE(root_v2 != root_fwd);
            REQUIRE(m_fwd.stats().cache_hits > 0);
            REQUIRE(hashed_delta < hashed_initial);
         }
      }
   }
}

SCENARIO("Merkleizer Bayyinah inclusion and non-inclusion proofs verify state and reject tampering")
{
   GIVEN("an authenticated CowArt state containing settled guild claims and prefix-overlapping keys")
   {
      ScopedDir        dir{"merkle_bayyinah"};
      SegmentAllocator sal{dir.path, SalConfig{.segment_bytes = 64 * 1024}};
      CowArt           art{sal};
      Merkleizer       merkle{art};

      // Verify empty-tree non-inclusion first
      ArtRoot     empty_root;
      Checksum256 empty_digest = merkle.compute_root(empty_root);
      auto        empty_absent = merkle.prove_non_inclusion(empty_root, bspan("claim/00001"));
      REQUIRE(Merkleizer::verify_non_inclusion(empty_digest, bspan("claim/00001"), empty_absent));

      // Verify single-entry inclusion & non-inclusion
      ArtRoot     single_root   = art.upsert(empty_root, bspan("claim/solo"), bspan("solo_val"));
      Checksum256 single_digest = merkle.compute_root(single_root);
      auto        single_inc    = merkle.prove_inclusion(single_root, bspan("claim/solo"));
      auto        single_abs    = merkle.prove_non_inclusion(single_root, bspan("claim/other"));
      REQUIRE(Merkleizer::verify_inclusion(single_digest, bspan("claim/solo"), bspan("solo_val"),
                                           single_inc));
      REQUIRE(Merkleizer::verify_non_inclusion(single_digest, bspan("claim/other"), single_abs));
      REQUIRE(!Merkleizer::verify_non_inclusion(single_digest, bspan("claim/solo"), single_abs));

      // Multi-branch state with prefix-terminal key ("claim/0001") + 120 claims
      ArtRoot root;
      root = art.upsert(root, bspan("claim/0001"), bspan("prefix_terminal_val"));
      for (int i = 0; i < 120; i += 2)  // even indices only, leaving odd indices absent!
      {
         root = art.upsert(root, bspan(pad_key(i)), bspan("settled_" + std::to_string(i)));
      }
      Checksum256 state_root = merkle.compute_root(root);

      WHEN("inclusion and non-inclusion proofs are generated across terminals, leaves, and gaps")
      {
         auto p_term = merkle.prove_inclusion(root, bspan("claim/0001"));
         auto p_even = merkle.prove_inclusion(root, bspan(pad_key(42)));

         auto abs_gap    = merkle.prove_non_inclusion(root, bspan(pad_key(43)));
         auto abs_before = merkle.prove_non_inclusion(root, bspan("aaa/before"));
         auto abs_short  = merkle.prove_non_inclusion(root, bspan("claim/00"));
         auto abs_ext    = merkle.prove_non_inclusion(root, bspan("claim/00042/extra"));

         THEN("all honest Bayyinah proofs verify and any tampered value, key, or root is rejected")
         {
            REQUIRE(Merkleizer::verify_inclusion(state_root, bspan("claim/0001"),
                                                 bspan("prefix_terminal_val"), p_term));
            REQUIRE(Merkleizer::verify_inclusion(state_root, bspan(pad_key(42)),
                                                 bspan("settled_42"), p_even));

            // Tampered value or key must fail verification
            REQUIRE(!Merkleizer::verify_inclusion(state_root, bspan(pad_key(42)),
                                                  bspan("settled_999_forged"), p_even));
            REQUIRE(!Merkleizer::verify_inclusion(state_root, bspan(pad_key(44)),
                                                  bspan("settled_42"), p_even));

            // Non-inclusion proofs across all structural cases verify cleanly
            REQUIRE(Merkleizer::verify_non_inclusion(state_root, bspan(pad_key(43)), abs_gap));
            REQUIRE(Merkleizer::verify_non_inclusion(state_root, bspan("aaa/before"), abs_before));
            REQUIRE(Merkleizer::verify_non_inclusion(state_root, bspan("claim/00"), abs_short));
            REQUIRE(
                Merkleizer::verify_non_inclusion(state_root, bspan("claim/00042/extra"), abs_ext));

            // Swapping a non-inclusion proof onto a different key or forging target_key onto
            // a present key (44) or sibling branch (53) must fail verification
            REQUIRE(!Merkleizer::verify_non_inclusion(state_root, bspan(pad_key(45)), abs_gap));

            MerkleProof forged_present = abs_gap;
            auto        k44            = pad_key(44);
            forged_present.target_key.assign(bspan(k44).begin(), bspan(k44).end());
            REQUIRE(!Merkleizer::verify_non_inclusion(state_root, bspan(k44), forged_present));

            MerkleProof forged_branch = abs_gap;
            auto        k53           = pad_key(53);
            forged_branch.target_key.assign(bspan(k53).begin(), bspan(k53).end());
            REQUIRE(!Merkleizer::verify_non_inclusion(state_root, bspan(k53), forged_branch));
         }
      }
   }
}

int main()
{
   return bdd_lite::run_all("MuabaseDB DWAL + Batch-Boundary Merkle Proofs BDD Suite");
}
