#include "bdd_lite.hpp"

#include <triedent/database.hpp>
#include <triedent/object_db.hpp>
#include <triedent/region_allocator.hpp>

#include <cstring>
#include <filesystem>
#include <mutex>

using namespace triedent;

namespace
{
   struct scoped_temp_dir
   {
      std::filesystem::path path;
      explicit scoped_temp_dir(std::string_view prefix)
      {
         static std::uint64_t counter = 0;
         path = std::filesystem::temp_directory_path() /
                (std::string(prefix) + "_" + std::to_string(::getpid()) + "_" +
                 std::to_string(++counter));
         std::filesystem::remove_all(path);
         std::filesystem::create_directories(path);
      }
      ~scoped_temp_dir()
      {
         std::error_code ec;
         std::filesystem::remove_all(path, ec);
      }
   };
}  // namespace

SCENARIO("object_db::gc_finish reclaims leaked orphans and rebuilds ascending free list")
{
   GIVEN("an object_db with retained, multiply-referenced, released, and leaked orphan IDs")
   {
      auto       tmp = std::filesystem::temp_directory_path();
      gc_queue   gc{64};
      object_db  ids{gc, tmp, open_mode::temporary};
      gc_session session{gc};

      object_id id1, id2, id3, id4;
      {
         std::unique_lock l{session};
         id1 = ids.alloc(l, node_type::bytes);  // live with ref=1
         id2 = ids.alloc(l, node_type::inner);  // leaked orphan (crash before release)
         id3 = ids.alloc(l, node_type::roots);  // live with ref=2
         id4 = ids.alloc(l, node_type::bytes);  // already released normally before crash

         ids.init(id1, {.offset = 64, .cache = hot_cache});
         ids.init(id2, {.offset = 128, .cache = hot_cache});
         ids.init(id3, {.offset = 256, .cache = warm_cache});
         ids.init(id4, {.offset = 512, .cache = hot_cache});

         ids.bump_count(id3);
         auto rel4 = ids.release(id4);
         REQUIRE(rel4.ref == 0);
      }

      WHEN("crash-recovery garbage collection retains only id1 (once) and id3 (twice)")
      {
         ids.gc_start();
         REQUIRE(ids.gc_retain(id1) == true);
         REQUIRE(ids.gc_retain(id3) == true);
         REQUIRE(ids.gc_retain(id3) == false);
         ids.gc_finish();

         THEN("live objects preserve their exact reference counts and locations")
         {
            REQUIRE(ids.ref(id1) == 1);
            REQUIRE(ids.get(id1).offset() == 64);
            REQUIRE(ids.ref(id3) == 2);
            REQUIRE(ids.get(id3).offset() == 256);
         }

         AND_THEN("leaked orphan id2 is reclaimed (ref == 0) and free list yields id2 then id4")
         {
            REQUIRE(ids.ref(id2) == 0);
            REQUIRE(ids.ref(id4) == 0);

            std::unique_lock l{session};
            auto             reused_first  = ids.alloc(l, node_type::bytes);
            auto             reused_second = ids.alloc(l, node_type::bytes);
            auto             fresh_fifth   = ids.alloc(l, node_type::bytes);

            REQUIRE(reused_first == id2);
            REQUIRE(reused_second == id4);
            REQUIRE(fresh_fifth.id == 5);
         }
      }
   }
}

SCENARIO("cache_allocator crash-recovery GC reclaims orphaned nodes so allocator becomes empty")
{
   GIVEN("a cache_allocator with an allocated node whose release was skipped by a crash")
   {
      scoped_temp_dir         dir{"triedent_gc_orphan"};
      cache_allocator::config cfg{
          .hot_bytes  = 64 * 1024,
          .warm_bytes = 64 * 1024,
          .cool_bytes = 64 * 1024,
          .cold_bytes = 64 * 1024,
      };
      cache_allocator ring{dir.path, cfg, open_mode::create};
      {
         auto             s = ring.start_session();
         std::unique_lock l{s};
         auto [lock, ptr] = ring.alloc(l, 64, node_type::bytes);
         std::memset(ptr, 0x5A, 64);
         // Intentionally drop lock without calling ring.release() to model crash leak
      }
      REQUIRE(!ring.is_empty());

      WHEN("crash-recovery GC runs with zero retained roots")
      {
         ring.gc_start();
         ring.gc_finish();

         THEN("all unretained objects have ref_count == 0 and ring.is_empty() is true")
         {
            REQUIRE(ring.is_empty());
         }
      }
   }
}

SCENARIO("region_allocator reopen preserves pending_write on non-zero current_region")
{
   GIVEN("a persistent region_allocator that has advanced to region 1 and been closed")
   {
      scoped_temp_dir        dir{"triedent_region_reopen"};
      auto                   ids_file  = dir.path / "obj_ids";
      auto                   cold_file = dir.path / "cold";
      std::vector<object_id> live_ids;

      {
         gc_queue         gc{64};
         object_db        ids{gc, ids_file, open_mode::create};
         region_allocator cold{gc, ids, cold_file, open_mode::create, 128};
         gc_session       session{gc};

         // Fill region 0 (128B) and advance current_region to 1:
         // each 40B payload -> ((40+7)&-8)+8 = 48B; 3 allocations = 144B -> 3rd lands in region 1
         for (int i = 0; i < 3; ++i)
         {
            std::unique_lock l{session};
            auto             id = ids.alloc(l, node_type::bytes);
            cold.try_allocate(l, id, 40,
                              [&](void* ptr, object_location loc)
                              {
                                 std::memset(ptr, 0x10 + i, 40);
                                 ids.init(id, loc);
                              });
            live_ids.push_back(id);
         }
      }

      WHEN("region_allocator is reopened in read_write mode and region 1 rolls over to region 2")
      {
         gc_queue         gc{64};
         object_db        ids{gc, ids_file, open_mode::read_write};
         region_allocator cold{gc, ids, cold_file, open_mode::read_write, 128};
         gc_session       session{gc};

         for (int i = 0; i < 3; ++i)
         {
            std::unique_lock l{session};
            auto             id = ids.alloc(l, node_type::bytes);
            cold.try_allocate(l, id, 40,
                              [&](void* ptr, object_location loc)
                              {
                                 std::memset(ptr, 0x30 + i, 40);
                                 ids.init(id, loc);
                              });
            live_ids.push_back(id);
         }

         // Release both objects in region 0 (live_ids[0] and live_ids[1]); region 0 had 32B dummy
         // tail already deallocated on rollover, so releasing the two 48B objects must free region 0!
         {
            std::lock_guard l{session};
            for (int i = 0; i < 2; ++i)
            {
               auto info = ids.release(live_ids[i]);
               cold.deallocate(info);
            }
         }
         for (int attempt = 0; attempt < 50; ++attempt)
         {
            gc.poll();
            if (cold.get_stats().available_bytes >= 128)
               break;
            std::this_thread::sleep_for(std::chrono::milliseconds(2));
         }

         THEN("current_region rollover succeeds and releasing region 0 objects frees region 0")
         {
            auto stats = cold.get_stats();
            REQUIRE(stats.available_bytes >= 128);
            REQUIRE(stats.used_bytes <= stats.total_bytes);

            std::lock_guard l{session};
            for (std::size_t i = 2; i < live_ids.size(); ++i)
            {
               auto           info = ids.get(live_ids[i]);
               object_header* h    = cold.get_object(info);
               REQUIRE(h->data_size() == 40);
            }
         }
      }
   }
}

SCENARIO("region_allocator small-region compaction evicts sparsest region instead of lowest index")
{
   GIVEN("region 0 at 56/128 bytes used and region 1 at 16/128 bytes used (threshold = 64)")
   {
      auto             tmp = std::filesystem::temp_directory_path();
      gc_queue         gc{64};
      object_db        ids{gc, tmp, open_mode::temporary};
      region_allocator cold{gc, ids, tmp, open_mode::temporary, 128};
      gc_session       session{gc};

      auto alloc_bytes = [&](std::uint32_t sz, std::uint8_t fill)
      {
         std::unique_lock l{session};
         auto             id = ids.alloc(l, node_type::bytes);
         cold.try_allocate(l, id, sz,
                           [&](void* ptr, object_location loc)
                           {
                              std::memset(ptr, fill, sz);
                              ids.init(id, loc);
                           });
         return id;
      };

      // Region 0: id_r0_keep (48B payload + 8B hdr = 56B) + id_r0_drop (64B payload + 8B hdr = 72B) = 128B
      auto id_r0_keep = alloc_bytes(48, 0xA1);
      auto id_r0_drop = alloc_bytes(64, 0xA2);
      REQUIRE(ids.get(id_r0_keep).offset() / 128 == 0);

      // Region 1: id_r1_keep (8B payload + 8B hdr = 16B) + id_r1_drop (104B payload + 8B hdr = 112B) = 128B
      auto id_r1_keep = alloc_bytes(8, 0xB1);
      auto id_r1_drop = alloc_bytes(104, 0xB2);
      REQUIRE(ids.get(id_r1_keep).offset() / 128 == 1);

      // Advance into Region 2 and fill most of it before releasing r0_drop and r1_drop
      auto id_r2_fill = alloc_bytes(112, 0xC1);  // 120B in region 2
      REQUIRE(ids.get(id_r2_fill).offset() / 128 == 2);

      // Now make Region 0 used=56 (<=64) and Region 1 used=16 (<=64); sum=72 > threshold(64)!
      {
         std::lock_guard l{session};
         auto            i0 = ids.release(id_r0_drop);
         cold.deallocate(i0);
         auto i1 = ids.release(id_r1_drop);
         cold.deallocate(i1);
      }

      WHEN("an allocation rolls over from Region 2 into Region 3, triggering queue_small_regions")
      {
         auto id_r3 = alloc_bytes(32, 0xD1);
         (void)id_r3;

         // Wait briefly for background evacuation thread to evacuate the queued sparse region
         for (int attempt = 0; attempt < 100; ++attempt)
         {
            gc.poll();
            if (ids.get(id_r1_keep).offset() / 128 != 1)
               break;
            std::this_thread::sleep_for(std::chrono::milliseconds(2));
         }

         THEN(
             "the sparser Region 1 (16B used) was evacuated rather than dropped in favor of Region "
             "0 (56B)")
         {
            REQUIRE(ids.get(id_r1_keep).offset() / 128 != 1);
            REQUIRE(ids.get(id_r0_keep).offset() / 128 == 0);
         }
      }
   }
}

SCENARIO("ring_allocator aligns unaligned ring capacity to prevent wrap-around header underflow")
{
   GIVEN("a ring_allocator constructed with non-8-byte-multiple size 132")
   {
      ring_allocator a{std::filesystem::temp_directory_path(), 132, 0, open_mode::temporary, false};
      gc_queue       gc{64};
      gc_session     session{gc};

      WHEN("allocations fill 128 bytes, swap advances, and a new allocation wraps around the end")
      {
         std::vector<std::uint64_t> offsets;
         for (int i = 0; i < 4; ++i)
         {
            std::unique_lock l{session};
            bool ok = a.try_allocate(l, object_id{static_cast<std::uint64_t>(i + 1)}, 24,
                                     [&](void* ptr, object_location loc)
                                     {
                                        offsets.push_back(loc.offset);
                                        std::memset(ptr, 0x40 + i, 24);
                                     });
            REQUIRE(ok);
         }

         std::size_t swapped_count = 0;
         gc.push(a.swap(96,
                        [&](object_header* o, object_location)
                        {
                           REQUIRE(o->size == 24);
                           ++swapped_count;
                           return true;
                        }));
         gc.poll();
         REQUIRE(swapped_count >= 2);

         // Next 24B allocation requires 32B used_size; with unaligned 132B ring, trailing_bytes=4 < 8!
         bool          wrapped_ok     = false;
         std::uint64_t wrapped_offset = 0;
         {
            std::unique_lock l{session};
            wrapped_ok = a.try_allocate(l, object_id{99}, 24,
                                        [&](void* ptr, object_location loc)
                                        {
                                           wrapped_offset = loc.offset;
                                           std::memset(ptr, 0x7F, 24);
                                        });
         }

         THEN(
             "wrap-around succeeds without underflowing dummy object_header size and swaps cleanly")
         {
            REQUIRE(wrapped_ok);
            auto* obj = a.get_object(wrapped_offset);
            REQUIRE(obj->size == 24);
            REQUIRE(obj->id == 99);

            bool saw_99 = false;
            gc.push(a.swap(128,
                           [&](object_header* o, object_location)
                           {
                              if (o->id == 99)
                              {
                                 REQUIRE(o->size == 24);
                                 saw_99 = true;
                              }
                              return true;
                           }));
            gc.poll();
            REQUIRE(saw_99);
         }
      }
   }
}

SCENARIO("node cloning and range splicing handle >64 roots and multi-branch nodes without VLAs")
{
   GIVEN("a triedent database storing a roots vector larger than 64 entries and spliced ranges")
   {
      scoped_temp_dir  dir{"triedent_no_vla"};
      database::config cfg{
          .hot_bytes  = 256 * 1024,
          .warm_bytes = 256 * 1024,
          .cool_bytes = 256 * 1024,
          .cold_bytes = 256 * 1024,
      };
      auto db = std::make_shared<database>(dir.path, cfg, open_mode::create);
      auto ws = db->start_write_session();

      auto as_span = [](std::string_view sv)
      { return std::span<const char>{sv.data(), sv.size()}; };

      std::vector<shared_root> subroots;
      for (int i = 0; i < 80; ++i)
      {
         shared_root sr;
         auto        k = "sub_" + std::to_string(i);
         auto        v = "val_" + std::to_string(i);
         ws->upsert(sr, as_span(k), as_span(v));
         subroots.push_back(std::move(sr));
      }

      WHEN("a >64-entry roots value is upserted, cloned across shared snapshots, and range-spliced")
      {
         shared_root r1;
         ws->upsert(r1, as_span("all_roots"), std::span<const shared_root>{subroots});
         auto r2 = r1;  // shared refcount=2 forces inner/value clone when mutating overlapping key
         ws->upsert(r2, as_span("all"), as_span("parent_prefix"));

         shared_root r_left;
         ws->upsert(r_left, as_span("k_a"), as_span("1"));
         ws->upsert(r_left, as_span("k_m"), as_span("left_m"));
         ws->upsert(r_left, as_span("k_z"), as_span("left_z"));
         shared_root r_right;
         ws->upsert(r_right, as_span("k_m"), as_span("right_m"));
         ws->upsert(r_right, as_span("k_z"), as_span("right_z"));
         ws->splice(r_left, r_right, as_span("k_f"), as_span("k_t"));

         THEN("all 80 roots survive cloning intact and spliced range reflects right-tree middle")
         {
            std::vector<shared_root> fetched;
            REQUIRE(ws->get(r2, as_span("all_roots"), nullptr, &fetched));
            REQUIRE(fetched.size() == 80);

            auto get_str = [&](const shared_root& root, std::string_view k) -> std::string
            {
               auto res = ws->get(root, as_span(k));
               REQUIRE(res.has_value());
               return std::string(res->begin(), res->end());
            };

            REQUIRE(get_str(fetched[0], "sub_0") == "val_0");
            REQUIRE(get_str(fetched[79], "sub_79") == "val_79");
            REQUIRE(get_str(r_left, "k_a") == "1");
            REQUIRE(get_str(r_left, "k_m") == "right_m");
            REQUIRE(get_str(r_left, "k_z") == "left_z");
         }
      }
   }
}

int main()
{
   return bdd_lite::run_all("Triedent Tech-Debt Regression Suite");
}
