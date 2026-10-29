#include "../../triedent/test/bdd_lite.hpp"

#include <muabase_db/sal.hpp>

#include <unistd.h>
#include <cstring>
#include <filesystem>
#include <string>
#include <string_view>
#include <thread>
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

   std::span<const std::byte> bytes_of(std::string_view sv)
   {
      return {reinterpret_cast<const std::byte*>(sv.data()), sv.size()};
   }

   std::string str_of(std::span<const std::byte> sp)
   {
      return {reinterpret_cast<const char*>(sp.data()), sp.size()};
   }
}  // namespace

SCENARIO(
    "SAL co-locates sibling ControlBlocks within 16-slot cacheline groups and supports >32K refs")
{
   GIVEN("a SegmentAllocator with fragmented allocations across multiple groups")
   {
      ScopedDir        dir{"sal_coloc"};
      SegmentAllocator sal{dir.path, SalConfig{.segment_bytes = 16 * 1024}};

      // Fill group 0 (slots 1..15) so next unhinted allocation lands in group 1 (slot 16)
      std::vector<PtrAddress> g0;
      for (int i = 0; i < 15; ++i)
      {
         auto a = sal.allocate(bytes_of("slot_" + std::to_string(i)), ObjectKind::ArtInner);
         REQUIRE(static_cast<bool>(a));
         REQUIRE(SegmentAllocator::sibling_group(a) == 0);
         g0.push_back(a);
      }

      auto parent_in_g1 = sal.allocate(bytes_of("parent_branch"), ObjectKind::ArtInner);
      REQUIRE(SegmentAllocator::sibling_group(parent_in_g1) == 1);

      // Free a slot in group 0 so global free-list has a low ID in group 0,
      // while parent_in_g1 asks for a sibling in group 1!
      REQUIRE(sal.release(g0[3]));
      sal.drain_deferred_releases();

      WHEN("sibling children are allocated with locality_hint = parent_in_g1 and retained 100,000x")
      {
         auto child_a =
             sal.allocate(bytes_of("child_branch_a"), ObjectKind::BTreeLeaf, parent_in_g1);
         auto child_b =
             sal.allocate(bytes_of("child_branch_b"), ObjectKind::BTreeLeaf, parent_in_g1);

         // Exceed Triedent's 15-bit (32,767) ceiling by >3x
         auto new_ref = sal.retain(child_a, 99'999);

         THEN("siblings share parent's 16-slot cacheline group and 21-bit refcount holds 100,000")
         {
            REQUIRE(SegmentAllocator::sibling_group(child_a) ==
                    SegmentAllocator::sibling_group(parent_in_g1));
            REQUIRE(SegmentAllocator::sibling_group(child_b) ==
                    SegmentAllocator::sibling_group(parent_in_g1));
            REQUIRE(new_ref == 100'000);
            REQUIRE(sal.ref_count(child_a) == 100'000);
            REQUIRE((sal.inspect_control(child_a).cacheline_offset() * kCachelineBytes) %
                        kCachelineBytes ==
                    0);
         }
      }
   }
}

SCENARIO("SAL deferred release queue reclaims deep 5,000-node chains in O(1) stack frames")
{
   GIVEN(
       "a linked chain of 5,000 Subtree objects that would blow the call stack if freed "
       "recursively")
   {
      ScopedDir        dir{"sal_deferred_gc"};
      SegmentAllocator sal{dir.path, SalConfig{.segment_bytes = 64 * 1024}};

      PtrAddress head{};
      for (int i = 0; i < 5000; ++i)
      {
         std::uint32_t child_raw = head.value;
         head = sal.allocate({reinterpret_cast<const std::byte*>(&child_raw), sizeof(child_raw)},
                             ObjectKind::Subtree);
      }
      REQUIRE(sal.stats().live_objects == 5000);

      WHEN("only the root is released and drained in bounded batches")
      {
         REQUIRE(sal.release(head));
         // Immediate release must enqueue only head without recursive cascade!
         REQUIRE(sal.stats().deferred_queue_depth == 1);

         auto enum_children = [](ObjectKind kind, std::span<const std::byte> payload,
                                 const std::function<void(PtrAddress)>& emit)
         {
            if (kind == ObjectKind::Subtree && payload.size() == sizeof(std::uint32_t))
            {
               std::uint32_t child_val = 0;
               std::memcpy(&child_val, payload.data(), sizeof(child_val));
               if (child_val != 0)
                  emit(PtrAddress{child_val});
            }
         };

         std::size_t first_batch = sal.drain_deferred_releases(100, enum_children);
         REQUIRE(first_batch == 100);
         REQUIRE(sal.stats().live_objects == 4900);

         std::size_t rest = sal.drain_deferred_releases(10'000, enum_children);
         REQUIRE(rest == 4900);

         THEN("all 5,000 objects are reclaimed iteratively with zero live leaks")
         {
            REQUIRE(sal.stats().live_objects == 0);
            REQUIRE(sal.stats().deferred_queue_depth == 0);
         }
      }
   }
}

SCENARIO(
    "SAL swizzled cold reads yield fibers, resist 1-shot scan thrash, and truncate pruned tail")
{
   GIVEN("sealed hot segments demoted to cold disk storage with a registered fiber yield hook")
   {
      ScopedDir        dir{"sal_cold_mfu"};
      SegmentAllocator sal{
          dir.path, SalConfig{.segment_bytes = 4096, .mfu_difficulty = 1, .enable_io_uring = true}};

      std::vector<PtrAddress> seg0_addrs;
      // Each ~512B object uses (16+512) -> 576B (9 cachelines); 7 objects fill 4096B segment 0
      // and roll over into active segment 1 so segment 0 is sealed!
      for (int i = 0; i < 9; ++i)
      {
         std::string payload(500, static_cast<char>('A' + i));
         seg0_addrs.push_back(sal.allocate(bytes_of(payload), ObjectKind::BTreeLeaf));
      }

      std::size_t demoted = sal.demote_sealed_hot_segments();
      REQUIRE(demoted >= 1);
      REQUIRE(!sal.inspect_control(seg0_addrs[0]).pinned());

      std::size_t yield_calls = 0;
      sal.set_fiber_yield_hook([&] { ++yield_calls; });

      WHEN("a cold object is scanned once vs touched a second time, and trailing segments empty")
      {
         // 1st cold read: must yield fiber, set MFU read_bit, and NOT promote on single-pass scan!
         auto v1 = sal.read(seg0_addrs[0],
                            [](ObjectKind, std::span<const std::byte> sp) { return str_of(sp); });
         REQUIRE(v1 == std::string(500, 'A'));
         REQUIRE(yield_calls == 1);
         REQUIRE(sal.inspect_control(seg0_addrs[0]).read_marked());
         REQUIRE(!sal.inspect_control(seg0_addrs[0]).pinned());
         REQUIRE(sal.stats().mfu_promotions == 0);

         // 2nd cold read (with mfu_difficulty = 1 -> p = 1/1^2 = 1.0): promotes to hot pinned RAM!
         auto v2 = sal.read(seg0_addrs[0],
                            [](ObjectKind, std::span<const std::byte> sp) { return str_of(sp); });
         REQUIRE(v2 == std::string(500, 'A'));
         REQUIRE(sal.inspect_control(seg0_addrs[0]).pinned());
         REQUIRE(sal.stats().mfu_promotions == 1);

         // 3rd read is now hot (~2ns direct pointer): zero additional fiber yields!
         auto yields_before_hot = yield_calls;
         auto v3 = sal.read(seg0_addrs[0],
                            [](ObjectKind, std::span<const std::byte> sp) { return str_of(sp); });
         REQUIRE(v3 == std::string(500, 'A'));
         REQUIRE(yield_calls == yields_before_hot);

         // Release remaining cold objects in segment 0 and prune tail of cold file!
         auto cold_bytes_before = sal.stats().cold_file_bytes;
         REQUIRE(cold_bytes_before >= 4096);
         for (std::size_t i = 1; i < seg0_addrs.size(); ++i)
         {
            sal.release(seg0_addrs[i]);
         }
         sal.drain_deferred_releases();
         std::size_t pruned = sal.prune_tail();

         THEN("pruned empty cold tail truncates the backing cold file back to 0 bytes")
         {
            REQUIRE(pruned >= 1);
            REQUIRE(sal.stats().cold_file_bytes == 0);
         }
      }
   }
}

int main()
{
   return bdd_lite::run_all("MuabaseDB SAL (Segment Allocator Library) BDD Suite");
}
