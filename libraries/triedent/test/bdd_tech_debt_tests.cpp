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

int main()
{
   return bdd_lite::run_all("Triedent Tech-Debt Regression Suite");
}
