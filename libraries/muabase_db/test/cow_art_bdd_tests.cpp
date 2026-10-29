#include "../../triedent/test/bdd_lite.hpp"

#include <muabase_db/cow_art.hpp>

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

   std::string pad_key(int n)
   {
      char buf[32];
      std::snprintf(buf, sizeof(buf), "acct/%05d", n);
      return buf;
   }
}  // namespace

SCENARIO("COW-ART preserves immutable historical snapshots across leaf splits and binary keys")
{
   GIVEN("an empty CowArt backed by SAL")
   {
      ScopedDir        dir{"art_cow_snapshots"};
      SegmentAllocator sal{dir.path, SalConfig{.segment_bytes = 64 * 1024}};
      CowArt           art{sal};

      // Include arbitrary 8-bit binary bytes (0x00, 0xFF) and prefix-overlapping keys
      std::string bin_k1{"\x00\xFF\x01key", 6};
      std::string bin_k2{"\x00\xFF\x01key_extended", 15};

      WHEN("500 keys split B-tree leaves into ART inner nodes across snapshot v1 and snapshot v2")
      {
         ArtRoot v1;
         v1 = art.upsert(v1, bspan(bin_k1), bspan("binary_v1"));
         v1 = art.upsert(v1, bspan(bin_k2), bspan("extended_v1"));
         for (int i = 0; i < 500; ++i)
         {
            v1 = art.upsert(v1, bspan(pad_key(i)), bspan("v1_" + std::to_string(i)));
         }

         // Fork v2 from v1, overwrite 50 keys, and delete 50 keys
         ArtRoot v2 = v1;
         for (int i = 0; i < 50; ++i)
         {
            v2 = art.upsert(v2, bspan(pad_key(i)), bspan("v2_updated_" + std::to_string(i)));
         }
         for (int i = 50; i < 100; ++i)
         {
            auto [next_root, removed] = art.remove(v2, bspan(pad_key(i)));
            REQUIRE(removed);
            v2 = std::move(next_root);
         }

         THEN("snapshot v1 remains 100% intact while v2 reflects updates and deletions")
         {
            REQUIRE(art.count(v1) == 502);
            REQUIRE(art.count(v2) == 452);

            auto r_bin1 = art.get(v1, bspan(bin_k1));
            auto r_bin2 = art.get(v1, bspan(bin_k2));
            REQUIRE(r_bin1.has_value() && sview(*r_bin1) == "binary_v1");
            REQUIRE(r_bin2.has_value() && sview(*r_bin2) == "extended_v1");

            auto old_0  = art.get(v1, bspan(pad_key(0)));
            auto new_0  = art.get(v2, bspan(pad_key(0)));
            auto old_75 = art.get(v1, bspan(pad_key(75)));
            auto new_75 = art.get(v2, bspan(pad_key(75)));
            REQUIRE(old_0.has_value() && sview(*old_0) == "v1_0");
            REQUIRE(new_0.has_value() && sview(*new_0) == "v2_updated_0");
            REQUIRE(old_75.has_value() && sview(*old_75) == "v1_75");
            REQUIRE(!new_75.has_value());
         }

         AND_THEN("dropping both snapshots and draining SAL reclaims 100% of live objects")
         {
            v1.reset();
            v2.reset();
            art.drain_garbage(100'000);
            REQUIRE(sal.stats().live_objects == 0);
         }
      }
   }
}

SCENARIO("COW-ART ordered cursors match psibase kvGreaterEqual, kvLessThan, and kvMax semantics")
{
   GIVEN("keys spanning multiple table prefixes and multi-level ART branches")
   {
      ScopedDir        dir{"art_ordered_cursors"};
      SegmentAllocator sal{dir.path, SalConfig{.segment_bytes = 64 * 1024}};
      CowArt           art{sal};

      ArtRoot root;
      for (int i = 10; i <= 90; i += 10)
      {
         root = art.upsert(root, bspan("tblA/" + std::to_string(i)),
                           bspan("A" + std::to_string(i)));
      }
      root = art.upsert(root, bspan("tblB/05"), bspan("B05"));

      WHEN("lower_bound, less_than, and max_with_prefix are queried with prefix bounds")
      {
         auto ge_exact = art.lower_bound(root, bspan("tblA/30"), 5);
         auto ge_mid   = art.lower_bound(root, bspan("tblA/35"), 5);
         auto ge_past  = art.lower_bound(root, bspan("tblA/95"), 5);  // must NOT bleed into tblB!

         auto lt_mid   = art.less_than(root, bspan("tblA/35"), 5);
         auto lt_first = art.less_than(root, bspan("tblA/10"), 5);
         auto lt_tblB  = art.less_than(root, bspan("tblB/99"), 5);

         auto max_A = art.max_with_prefix(root, bspan("tblA/"));
         auto max_C = art.max_with_prefix(root, bspan("tblC/"));

         THEN("results honor prefix boundaries and lexicographical byte order")
         {
            REQUIRE(ge_exact.has_value() && sview(ge_exact->key) == "tblA/30");
            REQUIRE(ge_mid.has_value() && sview(ge_mid->key) == "tblA/40");
            REQUIRE(!ge_past.has_value());

            REQUIRE(lt_mid.has_value() && sview(lt_mid->key) == "tblA/30");
            REQUIRE(!lt_first.has_value());
            REQUIRE(lt_tblB.has_value() && sview(lt_tblB->key) == "tblB/05");

            REQUIRE(max_A.has_value() && sview(max_A->key) == "tblA/90");
            REQUIRE(!max_C.has_value());
         }
      }
   }
}

SCENARIO("COW-ART computes count_range and remove_range in O(log N) via inner subtree counts")
{
   GIVEN("1,000 ordered keys demoted to cold segments so unvisited interior subtrees stay unread")
   {
      ScopedDir        dir{"art_range_ops"};
      SegmentAllocator sal{dir.path, SalConfig{.segment_bytes = 4096}};
      CowArt           art{sal};

      ArtRoot root;
      for (int i = 0; i < 1000; ++i)
      {
         root = art.upsert(root, bspan(pad_key(i)), bspan("payload_" + std::to_string(i)));
      }
      art.drain_garbage(100'000);
      sal.demote_sealed_hot_segments();

      WHEN("count_range and remove_range are executed over [acct/00200, acct/00800)")
      {
         auto cold_before = sal.stats().cold_reads;
         auto in_range    = art.count_range(root, bspan(pad_key(200)), bspan(pad_key(800)));
         auto cold_delta  = sal.stats().cold_reads - cold_before;

         ArtRoot pruned = art.remove_range(root, bspan(pad_key(200)), bspan(pad_key(800)));

         THEN("exact count is 600 with sublinear cold reads, and pruned snapshot retains 400 keys")
         {
            REQUIRE(in_range == 600);
            // 1,000 keys span >60 leaves, yet O(log N) subtree_count skips interior leaves!
            REQUIRE(cold_delta < 25);

            REQUIRE(art.count(pruned) == 400);
            REQUIRE(art.count_range(pruned, bspan(pad_key(200)), bspan(pad_key(800))) == 0);
            REQUIRE(art.get(pruned, bspan(pad_key(199))).has_value());
            REQUIRE(!art.get(pruned, bspan(pad_key(200))).has_value());
            REQUIRE(!art.get(pruned, bspan(pad_key(799))).has_value());
            REQUIRE(art.get(pruned, bspan(pad_key(800))).has_value());
         }
      }
   }
}

SCENARIO("COW-ART embeds reference-counted child subtrees and zero-copy leaf views without leaks")
{
   GIVEN("an outer CowArt root embedding two independent service subtrees")
   {
      ScopedDir        dir{"art_subtrees"};
      SegmentAllocator sal{dir.path, SalConfig{.segment_bytes = 64 * 1024}};
      CowArt           art{sal};

      ArtRoot lease_tree;
      lease_tree = art.upsert(lease_tree, bspan("lease/001"), bspan("active_tractor"));
      lease_tree = art.upsert(lease_tree, bspan("lease/002"), bspan("returned_plow"));

      ArtRoot credit_tree;
      credit_tree = art.upsert(credit_tree, bspan("edge/A->B"), bspan("500_MUA"));

      ArtRoot world;
      world = art.upsert_subtree(world, bspan("svc/lease-engine"), lease_tree);
      world = art.upsert_subtree(world, bspan("svc/credit-clear"), credit_tree);

      // Drop local handles; world alone keeps both subtrees alive via ValueTag::Subtree
      lease_tree.reset();
      credit_tree.reset();
      art.drain_garbage(10'000);

      WHEN("a service subtree is retrieved and inspected via zero-copy view")
      {
         auto reopened_lease = art.get_subtree(world, bspan("svc/lease-engine"));
         REQUIRE(reopened_lease.has_value());

         std::string viewed_payload;
         bool        viewed = art.view(*reopened_lease, bspan("lease/001"),
                                       [&](ValueTag tag, std::span<const std::byte> val)
                                       {
                                          REQUIRE(tag == ValueTag::Bytes);
                                          viewed_payload = sview(val);
                                       });

         THEN("child subtrees survive parent COW clones and release 100% of nodes when world drops")
         {
            REQUIRE(viewed);
            REQUIRE(viewed_payload == "active_tractor");
            REQUIRE(art.count(*reopened_lease) == 2);

            reopened_lease->reset();
            world.reset();
            art.drain_garbage(10'000);
            REQUIRE(sal.stats().live_objects == 0);
         }
      }
   }
}

int main()
{
   return bdd_lite::run_all("MuabaseDB COW-ART + Sorted B-Tree Leaf BDD Suite");
}
