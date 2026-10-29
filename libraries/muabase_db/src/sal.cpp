#include <muabase_db/sal.hpp>

#include <fcntl.h>
#include <linux/io_uring.h>
#include <sys/mman.h>
#include <sys/syscall.h>
#include <unistd.h>
#include <algorithm>
#include <bit>
#include <cassert>
#include <cerrno>
#include <cstdlib>
#include <cstring>
#include <deque>
#include <stdexcept>

namespace muabase_db
{
   namespace
   {
      constexpr std::size_t align_up(std::size_t n, std::size_t alignment) noexcept
      {
         return (n + alignment - 1) & ~(alignment - 1);
      }

      // Raw Linux io_uring async file reader with transparent POSIX pread fallback
      // when container seccomp policies restrict __NR_io_uring_setup.
      class AsyncColdReader
      {
        public:
         AsyncColdReader(int cold_fd, bool enable_uring) : _cold_fd(cold_fd)
         {
            if (!enable_uring || _cold_fd < 0)
               return;

            std::memset(&_params, 0, sizeof(_params));
            int fd = static_cast<int>(::syscall(__NR_io_uring_setup, 8u, &_params));
            if (fd < 0)
               return;

            _ring_fd    = fd;
            _sq_ring_sz = _params.sq_off.array + _params.sq_entries * sizeof(std::uint32_t);
            _cq_ring_sz = _params.cq_off.cqes + _params.cq_entries * sizeof(io_uring_cqe);
            if (_params.features & IORING_FEAT_SINGLE_MMAP)
            {
               _sq_ring_sz = std::max(_sq_ring_sz, _cq_ring_sz);
               _cq_ring_sz = _sq_ring_sz;
            }

            _sq_ptr = ::mmap(nullptr, _sq_ring_sz, PROT_READ | PROT_WRITE,
                             MAP_SHARED | MAP_POPULATE, _ring_fd, IORING_OFF_SQ_RING);
            if (_sq_ptr == MAP_FAILED)
            {
               _sq_ptr = nullptr;
               teardown();
               return;
            }

            if (_params.features & IORING_FEAT_SINGLE_MMAP)
            {
               _cq_ptr = _sq_ptr;
            }
            else
            {
               _cq_ptr = ::mmap(nullptr, _cq_ring_sz, PROT_READ | PROT_WRITE,
                                MAP_SHARED | MAP_POPULATE, _ring_fd, IORING_OFF_CQ_RING);
               if (_cq_ptr == MAP_FAILED)
               {
                  _cq_ptr = nullptr;
                  teardown();
                  return;
               }
            }

            _sqes_sz = _params.sq_entries * sizeof(io_uring_sqe);
            _sqes    = static_cast<io_uring_sqe*>(::mmap(nullptr, _sqes_sz, PROT_READ | PROT_WRITE,
                                                         MAP_SHARED | MAP_POPULATE, _ring_fd,
                                                         IORING_OFF_SQES));
            if (_sqes == MAP_FAILED)
            {
               _sqes = nullptr;
               teardown();
               return;
            }
            _active = true;
         }

         ~AsyncColdReader() { teardown(); }

         bool active() const noexcept { return _active; }

         void read_exact(std::uint64_t         file_offset,
                         std::byte*            dst,
                         std::size_t           len,
                         const FiberYieldHook& yield_hook)
         {
            if (_active)
            {
               std::lock_guard lk{_ring_mutex};
               auto*           sq_Base = static_cast<std::uint8_t*>(_sq_ptr);
               auto* sq_tail = reinterpret_cast<std::uint32_t*>(sq_Base + _params.sq_off.tail);
               auto* sq_mask = reinterpret_cast<std::uint32_t*>(sq_Base + _params.sq_off.ring_mask);
               auto* sq_array = reinterpret_cast<std::uint32_t*>(sq_Base + _params.sq_off.array);

               auto* cq_base = static_cast<std::uint8_t*>(_cq_ptr);
               auto* cq_head = reinterpret_cast<std::uint32_t*>(cq_base + _params.cq_off.head);
               auto* cq_tail = reinterpret_cast<std::uint32_t*>(cq_base + _params.cq_off.tail);
               auto* cq_mask = reinterpret_cast<std::uint32_t*>(cq_base + _params.cq_off.ring_mask);
               auto* cqes    = reinterpret_cast<io_uring_cqe*>(cq_base + _params.cq_off.cqes);

               std::uint32_t tail  = *sq_tail;
               std::uint32_t index = tail & *sq_mask;
               io_uring_sqe& sqe   = _sqes[index];
               std::memset(&sqe, 0, sizeof(sqe));
               sqe.opcode    = IORING_OP_READ;
               sqe.fd        = _cold_fd;
               sqe.off       = file_offset;
               sqe.addr      = reinterpret_cast<std::uint64_t>(dst);
               sqe.len       = static_cast<std::uint32_t>(len);
               sqe.user_data = ++_req_id;

               sq_array[index] = index;
               std::atomic_ref<std::uint32_t>(*sq_tail).store(tail + 1, std::memory_order_release);

               // Submit SQE asynchronously without blocking, yield fiber, then harvest CQE
               long submitted = ::syscall(__NR_io_uring_enter, _ring_fd, 1u, 0u, 0u, nullptr, 0);
               if (submitted >= 0)
               {
                  if (yield_hook)
                     yield_hook();

                  while (std::atomic_ref<std::uint32_t>(*cq_head).load(std::memory_order_acquire) ==
                         std::atomic_ref<std::uint32_t>(*cq_tail).load(std::memory_order_acquire))
                  {
                     long rc = ::syscall(__NR_io_uring_enter, _ring_fd, 0u, 1u,
                                         IORING_ENTER_GETEVENTS, nullptr, 0);
                     if (rc < 0 && errno != EINTR)
                        break;
                  }

                  std::uint32_t head =
                      std::atomic_ref<std::uint32_t>(*cq_head).load(std::memory_order_relaxed);
                  if (head !=
                      std::atomic_ref<std::uint32_t>(*cq_tail).load(std::memory_order_acquire))
                  {
                     const io_uring_cqe& cqe = cqes[head & *cq_mask];
                     std::int32_t        res = cqe.res;
                     std::atomic_ref<std::uint32_t>(*cq_head).store(head + 1,
                                                                    std::memory_order_release);
                     if (res == static_cast<std::int32_t>(len))
                        return;
                  }
               }
            }
            else if (yield_hook)
            {
               yield_hook();
            }

            // Fallback / short-read completion via pread
            std::size_t done = 0;
            while (done < len)
            {
               ssize_t r = ::pread(_cold_fd, dst + done, len - done,
                                   static_cast<off_t>(file_offset + done));
               if (r <= 0)
                  throw std::runtime_error("SAL cold segment pread failed");
               done += static_cast<std::size_t>(r);
            }
         }

        private:
         void teardown() noexcept
         {
            if (_sqes && _sqes != MAP_FAILED)
               ::munmap(_sqes, _sqes_sz);
            if (_cq_ptr && _cq_ptr != MAP_FAILED && _cq_ptr != _sq_ptr)
               ::munmap(_cq_ptr, _cq_ring_sz);
            if (_sq_ptr && _sq_ptr != MAP_FAILED)
               ::munmap(_sq_ptr, _sq_ring_sz);
            if (_ring_fd >= 0)
               ::close(_ring_fd);
            _sqes    = nullptr;
            _cq_ptr  = nullptr;
            _sq_ptr  = nullptr;
            _ring_fd = -1;
            _active  = false;
         }

         int             _cold_fd    = -1;
         int             _ring_fd    = -1;
         bool            _active     = false;
         io_uring_params _params     = {};
         void*           _sq_ptr     = nullptr;
         void*           _cq_ptr     = nullptr;
         io_uring_sqe*   _sqes       = nullptr;
         std::size_t     _sq_ring_sz = 0;
         std::size_t     _cq_ring_sz = 0;
         std::size_t     _sqes_sz    = 0;
         std::uint64_t   _req_id     = 0;
         std::mutex      _ring_mutex;
      };

      struct HotBufferDeleter
      {
         void operator()(std::byte* p) const noexcept { std::free(p); }
      };
      using HotBufferPtr = std::unique_ptr<std::byte, HotBufferDeleter>;

      HotBufferPtr make_aligned_buffer(std::size_t bytes)
      {
         std::size_t aligned = align_up(bytes, kCachelineBytes);
         void*       raw     = std::aligned_alloc(kCachelineBytes, aligned);
         if (!raw)
            throw std::bad_alloc();
         std::memset(raw, 0, aligned);
         return HotBufferPtr{static_cast<std::byte*>(raw)};
      }
   }  // namespace

   struct SegmentAllocator::Impl
   {
      static constexpr std::size_t kMaxControlSlots = 1u << 20;  // 1M slots (8 MiB demand-paged)
      static constexpr std::size_t kMaxSegments     = 1u << 16;  // 65,536 segments

      struct SegmentSlot
      {
         std::atomic<std::byte*>    hot_data{nullptr};
         std::atomic<bool>          in_use{false};
         std::atomic<bool>          is_cold{false};
         std::atomic<bool>          is_sealed{false};
         std::atomic<std::size_t>   write_cachelines{0};
         std::atomic<std::int64_t>  live_cachelines{0};
         std::atomic<std::uint64_t> cold_file_offset{0};
      };

      std::filesystem::path            dir;
      SalConfig                        cfg;
      std::size_t                      segment_bytes      = 0;
      std::size_t                      segment_cachelines = 0;
      int                              cold_fd            = -1;
      std::unique_ptr<AsyncColdReader> cold_reader;

      std::atomic<std::uint64_t>* controls        = nullptr;
      std::size_t                 ctrl_mmap_bytes = 0;

      std::vector<SegmentSlot>   segments{kMaxSegments};
      std::vector<HotBufferPtr>  owned_hot_buffers;
      std::vector<HotBufferPtr>  retired_hot_buffers;
      std::vector<std::uint32_t> cold_segment_order;  // ordered by cold_file_offset

      mutable std::mutex        alloc_mutex;
      std::uint32_t             active_hot_seg = 0;
      std::uint32_t             next_seg_id    = 0;
      std::uint32_t             high_water_id  = 0;  // slot 0 reserved as null
      std::deque<std::uint32_t> free_ids;

      mutable std::mutex     deferred_mutex;
      std::deque<PtrAddress> deferred_queue;

      mutable std::shared_mutex hook_mutex;
      FiberYieldHook            yield_hook;

      std::atomic<std::size_t>   live_objects{0};
      std::atomic<std::uint64_t> cold_file_bytes{0};
      std::atomic<std::uint64_t> cold_reads{0};
      std::atomic<std::uint64_t> mfu_promotions{0};
      std::atomic<std::uint64_t> fiber_yields{0};
      std::atomic<std::uint32_t> mfu_difficulty{1};
      std::atomic<std::uint64_t> rng_state{0x9E3779B97F4A7C15ull};

      Impl(const std::filesystem::path& d, SalConfig c) : dir(d), cfg(c)
      {
         std::filesystem::create_directories(dir);
         std::size_t min_seg = std::max<std::size_t>(cfg.segment_bytes, 4096);
         segment_cachelines  = std::bit_ceil(align_up(min_seg, kCachelineBytes) / kCachelineBytes);
         segment_bytes       = segment_cachelines * kCachelineBytes;
         mfu_difficulty.store(std::max(1u, cfg.mfu_difficulty), std::memory_order_relaxed);

         ctrl_mmap_bytes = kMaxControlSlots * sizeof(std::atomic<std::uint64_t>);
         void* mem       = ::mmap(nullptr, ctrl_mmap_bytes, PROT_READ | PROT_WRITE,
                                  MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
         if (mem == MAP_FAILED)
            throw std::bad_alloc();
         controls = static_cast<std::atomic<std::uint64_t>*>(mem);

         auto cold_path = dir / "segments.cold";
         cold_fd        = ::open(cold_path.c_str(), O_RDWR | O_CREAT | O_TRUNC | O_CLOEXEC, 0644);
         if (cold_fd < 0)
            throw std::runtime_error("Failed to open SAL cold segment file: " + cold_path.string());

         cold_reader    = std::make_unique<AsyncColdReader>(cold_fd, cfg.enable_io_uring);
         active_hot_seg = create_hot_segment_locked();
      }

      ~Impl()
      {
         cold_reader.reset();
         if (cold_fd >= 0)
            ::close(cold_fd);
         if (controls)
            ::munmap(controls, ctrl_mmap_bytes);
      }

      std::uint32_t create_hot_segment_locked()
      {
         if (next_seg_id >= kMaxSegments)
            throw std::runtime_error("SAL segment table exhausted");
         std::uint32_t sid = next_seg_id++;
         auto          buf = make_aligned_buffer(segment_bytes);
         segments[sid].hot_data.store(buf.get(), std::memory_order_release);
         segments[sid].write_cachelines.store(0, std::memory_order_relaxed);
         segments[sid].live_cachelines.store(0, std::memory_order_relaxed);
         segments[sid].is_cold.store(false, std::memory_order_relaxed);
         segments[sid].is_sealed.store(false, std::memory_order_relaxed);
         segments[sid].in_use.store(true, std::memory_order_release);
         owned_hot_buffers.push_back(std::move(buf));
         return sid;
      }

      std::pair<std::uint32_t, std::size_t> reserve_hot_space_locked(std::size_t needed_cl)
      {
         if (needed_cl > segment_cachelines)
            throw std::runtime_error("SAL object exceeds segment_bytes capacity");

         auto& cur  = segments[active_hot_seg];
         auto  wpos = cur.write_cachelines.load(std::memory_order_relaxed);
         if (wpos + needed_cl > segment_cachelines)
         {
            cur.is_sealed.store(true, std::memory_order_release);
            active_hot_seg = create_hot_segment_locked();
            wpos           = 0;
         }
         segments[active_hot_seg].write_cachelines.store(wpos + needed_cl,
                                                         std::memory_order_relaxed);
         segments[active_hot_seg].live_cachelines.fetch_add(static_cast<std::int64_t>(needed_cl),
                                                            std::memory_order_relaxed);
         return {active_hot_seg, wpos};
      }

      std::uint32_t claim_control_slot_locked(PtrAddress hint)
      {
         // 1. Try 16-slot cacheline-co-located sibling group first when locality_hint is set!
         if (hint)
         {
            std::uint32_t group_base = (hint.value / kSiblingGroupSlots) * kSiblingGroupSlots;
            std::uint32_t start      = std::max<std::uint32_t>(1u, group_base);
            std::uint32_t end        = std::min<std::uint32_t>(
                static_cast<std::uint32_t>(kMaxControlSlots),
                group_base + static_cast<std::uint32_t>(kSiblingGroupSlots));
            for (std::uint32_t cand = start; cand < end; ++cand)
            {
               if (cand > high_water_id)
               {
                  // Advance high_water_id up to cand, placing skipped intermediate slots on free_ids
                  while (high_water_id + 1 < cand)
                  {
                     ++high_water_id;
                     free_ids.push_back(high_water_id);
                  }
                  high_water_id = cand;
                  return cand;
               }
               if (controls[cand].load(std::memory_order_acquire) == 0)
               {
                  auto it = std::find(free_ids.begin(), free_ids.end(), cand);
                  if (it != free_ids.end())
                  {
                     free_ids.erase(it);
                     return cand;
                  }
               }
            }
         }

         // 2. Otherwise pop from free list or bump high_water_id
         while (!free_ids.empty())
         {
            std::uint32_t id = free_ids.front();
            free_ids.pop_front();
            if (controls[id].load(std::memory_order_acquire) == 0)
               return id;
         }

         if (high_water_id + 1 >= kMaxControlSlots)
            throw std::runtime_error("SAL control table exhausted");
         return ++high_water_id;
      }

      bool should_promote_mfu() noexcept
      {
         std::uint64_t d = mfu_difficulty.load(std::memory_order_relaxed);
         if (d <= 1)
            return true;
         std::uint64_t x = rng_state.fetch_add(0x9E3779B97F4A7C15ull, std::memory_order_relaxed);
         x               = (x ^ (x >> 30)) * 0xBF58476D1CE4E5B9ull;
         x               = (x ^ (x >> 27)) * 0x94D049BB133111EBull;
         x               = x ^ (x >> 31);
         return (x % (d * d)) == 0;
      }
   };

   SegmentAllocator::SegmentAllocator(const std::filesystem::path& dir, SalConfig cfg)
       : _impl(std::make_unique<Impl>(dir, cfg))
   {
   }

   SegmentAllocator::~SegmentAllocator() = default;

   PtrAddress SegmentAllocator::allocate(std::span<const std::byte> payload,
                                         ObjectKind                 kind,
                                         PtrAddress                 locality_hint)
   {
      const std::size_t total_bytes = sizeof(AllocHeader) + payload.size();
      const std::size_t needed_cl   = align_up(total_bytes, kCachelineBytes) / kCachelineBytes;

      std::lock_guard lk{_impl->alloc_mutex};
      std::uint32_t   id        = _impl->claim_control_slot_locked(locality_hint);
      auto [seg_id, seg_cl_idx] = _impl->reserve_hot_space_locked(needed_cl);
      std::byte* seg_base       = _impl->segments[seg_id].hot_data.load(std::memory_order_relaxed);
      std::byte* obj_ptr        = seg_base + seg_cl_idx * kCachelineBytes;

      auto* hdr           = new (obj_ptr) AllocHeader{};
      hdr->magic_and_kind = AllocHeader::pack_magic(kind);
      hdr->payload_size   = static_cast<std::uint32_t>(payload.size());
      hdr->address        = id;
      hdr->flags          = 0;

      if (!payload.empty())
         std::memcpy(obj_ptr + sizeof(AllocHeader), payload.data(), payload.size());

      std::uint64_t global_cl =
          static_cast<std::uint64_t>(seg_id) * _impl->segment_cachelines + seg_cl_idx;
      auto cw = ControlWord::pack(/*ref=*/1u, global_cl, /*is_pinned=*/true, /*is_read=*/false);
      _impl->controls[id].store(cw.raw, std::memory_order_release);
      _impl->live_objects.fetch_add(1, std::memory_order_relaxed);
      return PtrAddress{id};
   }

   std::uint32_t SegmentAllocator::retain(PtrAddress addr, std::uint32_t delta)
   {
      if (!addr || addr.value >= Impl::kMaxControlSlots)
         throw std::out_of_range("SAL retain on invalid PtrAddress");
      if (delta == 0)
         return ref_count(addr);

      auto&         slot = _impl->controls[addr.value];
      std::uint64_t cur  = slot.load(std::memory_order_relaxed);
      while (true)
      {
         ControlWord   cw{cur};
         std::uint32_t r = cw.ref_count();
         if (r == 0)
            throw std::runtime_error("SAL retain on released object");
         if (r + delta > kMaxRefCount)
            throw std::overflow_error("SAL 21-bit ref_count overflow");
         std::uint64_t next =
             (cur & ~ControlWord::kRefMask) | static_cast<std::uint64_t>(r + delta);
         if (slot.compare_exchange_weak(cur, next, std::memory_order_acq_rel,
                                        std::memory_order_relaxed))
         {
            return r + delta;
         }
      }
   }

   std::uint32_t SegmentAllocator::ref_count(PtrAddress addr) const noexcept
   {
      return inspect_control(addr).ref_count();
   }

   ControlWord SegmentAllocator::inspect_control(PtrAddress addr) const noexcept
   {
      if (!addr || addr.value >= Impl::kMaxControlSlots)
         return {};
      return ControlWord{_impl->controls[addr.value].load(std::memory_order_acquire)};
   }

   bool SegmentAllocator::release(PtrAddress addr)
   {
      if (!addr || addr.value >= Impl::kMaxControlSlots)
         return false;

      auto&         slot = _impl->controls[addr.value];
      std::uint64_t cur  = slot.load(std::memory_order_acquire);
      while (true)
      {
         ControlWord   cw{cur};
         std::uint32_t r = cw.ref_count();
         if (r == 0)
            throw std::runtime_error("SAL double-release on zero ref_count");
         if (r > 1)
         {
            std::uint64_t next = (cur & ~ControlWord::kRefMask) | static_cast<std::uint64_t>(r - 1);
            if (slot.compare_exchange_weak(cur, next, std::memory_order_acq_rel,
                                           std::memory_order_acquire))
            {
               return false;
            }
         }
         else
         {
            // Keep location bits intact until drain_deferred_releases harvests children + size;
            // zero only ref_count bits to mark logically dead, then enqueue O(1) without recursion!
            std::uint64_t zombie = cur & ~ControlWord::kRefMask;
            if (slot.compare_exchange_weak(cur, zombie, std::memory_order_acq_rel,
                                           std::memory_order_acquire))
            {
               std::lock_guard qlk{_impl->deferred_mutex};
               _impl->deferred_queue.push_back(addr);
               return true;
            }
         }
      }
   }

   std::size_t SegmentAllocator::drain_deferred_releases(std::size_t              max_steps,
                                                         const ChildEnumeratorFn& child_enumerator)
   {
      std::size_t            drained = 0;
      std::vector<std::byte> cold_scratch;

      while (drained < max_steps)
      {
         PtrAddress addr{};
         {
            std::lock_guard qlk{_impl->deferred_mutex};
            if (_impl->deferred_queue.empty())
               break;
            addr = _impl->deferred_queue.front();
            _impl->deferred_queue.pop_front();
         }

         ControlWord   cw{_impl->controls[addr.value].load(std::memory_order_acquire)};
         std::uint64_t cl_off     = cw.cacheline_offset();
         std::uint32_t seg_id     = static_cast<std::uint32_t>(cl_off / _impl->segment_cachelines);
         std::size_t   seg_cl_idx = static_cast<std::size_t>(cl_off % _impl->segment_cachelines);

         const AllocHeader* hdr     = nullptr;
         const std::byte*   payload = nullptr;
         if (cw.pinned())
         {
            std::byte* base = _impl->segments[seg_id].hot_data.load(std::memory_order_acquire);
            hdr     = reinterpret_cast<const AllocHeader*>(base + seg_cl_idx * kCachelineBytes);
            payload = base + seg_cl_idx * kCachelineBytes + sizeof(AllocHeader);
         }
         else
         {
            std::uint64_t file_off =
                _impl->segments[seg_id].cold_file_offset.load(std::memory_order_acquire) +
                seg_cl_idx * kCachelineBytes;
            cold_scratch.resize(kCachelineBytes);
            _impl->cold_reader->read_exact(file_off, cold_scratch.data(), kCachelineBytes, {});
            auto*       h0      = reinterpret_cast<const AllocHeader*>(cold_scratch.data());
            std::size_t full_sz = align_up(sizeof(AllocHeader) + h0->payload_size, kCachelineBytes);
            if (full_sz > kCachelineBytes && child_enumerator)
            {
               cold_scratch.resize(full_sz);
               _impl->cold_reader->read_exact(file_off + kCachelineBytes,
                                              cold_scratch.data() + kCachelineBytes,
                                              full_sz - kCachelineBytes, {});
            }
            hdr     = reinterpret_cast<const AllocHeader*>(cold_scratch.data());
            payload = cold_scratch.data() + sizeof(AllocHeader);
         }

         std::size_t used_cl =
             align_up(sizeof(AllocHeader) + hdr->payload_size, kCachelineBytes) / kCachelineBytes;

         if (child_enumerator)
         {
            auto emit_child = [this](PtrAddress child)
            {
               if (child)
                  release(child);
            };
            child_enumerator(hdr->kind(), {payload, hdr->payload_size}, emit_child);
         }

         _impl->segments[seg_id].live_cachelines.fetch_sub(static_cast<std::int64_t>(used_cl),
                                                           std::memory_order_acq_rel);
         _impl->controls[addr.value].store(0, std::memory_order_release);
         {
            std::lock_guard alk{_impl->alloc_mutex};
            _impl->free_ids.push_front(addr.value);
         }
         _impl->live_objects.fetch_sub(1, std::memory_order_relaxed);
         ++drained;
      }
      return drained;
   }

   std::span<const std::byte> SegmentAllocator::acquire_view(
       PtrAddress                             addr,
       ObjectKind&                            out_kind,
       std::optional<std::vector<std::byte>>& cold_buf)
   {
      if (!addr || addr.value >= Impl::kMaxControlSlots)
         throw std::out_of_range("SAL read on invalid PtrAddress");

      auto&         slot = _impl->controls[addr.value];
      std::uint64_t raw  = slot.load(std::memory_order_acquire);
      ControlWord   cw{raw};
      if (cw.ref_count() == 0)
         throw std::runtime_error("SAL read on released PtrAddress");

      // Fast path: pinned/resident hot segment (~2ns direct pointer dereference, zero syscalls)
      if (cw.pinned())
      {
         std::uint64_t cl_off     = cw.cacheline_offset();
         std::uint32_t seg_id     = static_cast<std::uint32_t>(cl_off / _impl->segment_cachelines);
         std::size_t   seg_cl_idx = static_cast<std::size_t>(cl_off % _impl->segment_cachelines);
         std::byte*    base = _impl->segments[seg_id].hot_data.load(std::memory_order_acquire);
         const auto*   hdr =
             reinterpret_cast<const AllocHeader*>(base + seg_cl_idx * kCachelineBytes);
         assert(hdr->valid());
         out_kind = hdr->kind();
         return {base + seg_cl_idx * kCachelineBytes + sizeof(AllocHeader), hdr->payload_size};
      }

      // Cold path: swizzled async io_uring / pread read + fiber yield hook
      FiberYieldHook hook_copy;
      {
         std::shared_lock hlk{_impl->hook_mutex};
         hook_copy = _impl->yield_hook;
      }
      auto wrapped_yield = [&]()
      {
         _impl->fiber_yields.fetch_add(1, std::memory_order_relaxed);
         if (hook_copy)
            hook_copy();
      };

      std::uint64_t cl_off     = cw.cacheline_offset();
      std::uint32_t cold_seg   = static_cast<std::uint32_t>(cl_off / _impl->segment_cachelines);
      std::size_t   seg_cl_idx = static_cast<std::size_t>(cl_off % _impl->segment_cachelines);
      std::uint64_t file_off =
          _impl->segments[cold_seg].cold_file_offset.load(std::memory_order_acquire) +
          seg_cl_idx * kCachelineBytes;

      cold_buf.emplace(kCachelineBytes);
      _impl->cold_reader->read_exact(file_off, cold_buf->data(), kCachelineBytes, wrapped_yield);
      _impl->cold_reads.fetch_add(1, std::memory_order_relaxed);

      const auto* hdr0 = reinterpret_cast<const AllocHeader*>(cold_buf->data());
      if (!hdr0->valid())
         throw std::runtime_error("Corrupt AllocHeader in SAL cold segment");

      std::size_t total_aligned =
          align_up(sizeof(AllocHeader) + hdr0->payload_size, kCachelineBytes);
      if (total_aligned > kCachelineBytes)
      {
         cold_buf->resize(total_aligned);
         // Complete remaining cachelines without double-counting the fiber yield of this logical read
         _impl->cold_reader->read_exact(file_off + kCachelineBytes,
                                        cold_buf->data() + kCachelineBytes,
                                        total_aligned - kCachelineBytes, {});
      }

      const auto* hdr = reinterpret_cast<const AllocHeader*>(cold_buf->data());
      out_kind        = hdr->kind();

      // Scan-resistant MFU policy:
      //   1st touch -> set read_bit (bit 63), do NOT promote single-scan cold items!
      //   2nd+ touch -> promote to hot resident segment with probability 1 / difficulty^2.
      if (!cw.read_marked())
      {
         std::uint64_t expected = raw;
         while (ControlWord{expected}.ref_count() > 0 && !ControlWord{expected}.pinned() &&
                !ControlWord{expected}.read_marked())
         {
            if (slot.compare_exchange_weak(expected, expected | ControlWord::kReadBit,
                                           std::memory_order_acq_rel, std::memory_order_acquire))
               break;
         }
      }
      else if (_impl->should_promote_mfu())
      {
         std::size_t     needed_cl = total_aligned / kCachelineBytes;
         std::lock_guard alk{_impl->alloc_mutex};
         std::uint64_t   cur = slot.load(std::memory_order_acquire);
         ControlWord     cur_cw{cur};
         if (cur_cw.ref_count() > 0 && !cur_cw.pinned() && cur_cw.cacheline_offset() == cl_off)
         {
            auto [hot_seg, hot_cl_idx] = _impl->reserve_hot_space_locked(needed_cl);
            std::byte* dst = _impl->segments[hot_seg].hot_data.load(std::memory_order_relaxed) +
                             hot_cl_idx * kCachelineBytes;
            std::memcpy(dst, cold_buf->data(), total_aligned);

            std::uint64_t hot_global_cl =
                static_cast<std::uint64_t>(hot_seg) * _impl->segment_cachelines + hot_cl_idx;
            while (cur_cw.ref_count() > 0 && !cur_cw.pinned() &&
                   cur_cw.cacheline_offset() == cl_off)
            {
               auto next = ControlWord::pack(cur_cw.ref_count(), hot_global_cl, /*is_pinned=*/true,
                                             /*is_read=*/true);
               if (slot.compare_exchange_weak(cur, next.raw, std::memory_order_acq_rel,
                                              std::memory_order_acquire))
               {
                  _impl->segments[cold_seg].live_cachelines.fetch_sub(
                      static_cast<std::int64_t>(needed_cl), std::memory_order_acq_rel);
                  _impl->mfu_promotions.fetch_add(1, std::memory_order_relaxed);
                  break;
               }
               cur_cw = ControlWord{cur};
            }
         }
      }

      return {cold_buf->data() + sizeof(AllocHeader), hdr->payload_size};
   }

   std::size_t SegmentAllocator::demote_sealed_hot_segments()
   {
      std::lock_guard lk{_impl->alloc_mutex};
      std::size_t     demoted = 0;

      for (std::uint32_t sid = 0; sid < _impl->next_seg_id; ++sid)
      {
         auto& seg = _impl->segments[sid];
         if (!seg.in_use.load(std::memory_order_relaxed) ||
             seg.is_cold.load(std::memory_order_relaxed) ||
             !seg.is_sealed.load(std::memory_order_relaxed) || sid == _impl->active_hot_seg)
         {
            continue;
         }

         std::byte*  hot_ptr  = seg.hot_data.load(std::memory_order_acquire);
         std::size_t written  = seg.write_cachelines.load(std::memory_order_acquire);
         std::size_t byte_len = written * kCachelineBytes;
         if (seg.live_cachelines.load(std::memory_order_acquire) <= 0 || byte_len == 0)
         {
            seg.in_use.store(false, std::memory_order_release);
            continue;
         }

         std::uint64_t cold_off = _impl->cold_file_bytes.load(std::memory_order_relaxed);
         std::size_t   done     = 0;
         while (done < byte_len)
         {
            ssize_t w = ::pwrite(_impl->cold_fd, hot_ptr + done, byte_len - done,
                                 static_cast<off_t>(cold_off + done));
            if (w <= 0)
               throw std::runtime_error("SAL cold segment pwrite failed");
            done += static_cast<std::size_t>(w);
         }
         _impl->cold_file_bytes.store(cold_off + _impl->segment_bytes, std::memory_order_release);
         if (::ftruncate(_impl->cold_fd, static_cast<off_t>(cold_off + _impl->segment_bytes)) != 0)
         {
            throw std::runtime_error("SAL cold segment ftruncate expand failed");
         }

         seg.cold_file_offset.store(cold_off, std::memory_order_release);
         seg.is_cold.store(true, std::memory_order_release);
         _impl->cold_segment_order.push_back(sid);

         // Walk segment objects and clear kPinnedBit atomically on every live ControlBlock
         std::size_t cl = 0;
         while (cl < written)
         {
            const auto* hdr = reinterpret_cast<const AllocHeader*>(hot_ptr + cl * kCachelineBytes);
            if (!hdr->valid())
               break;
            std::size_t obj_cl =
                align_up(sizeof(AllocHeader) + hdr->payload_size, kCachelineBytes) /
                kCachelineBytes;
            std::uint64_t expected_cl_off =
                static_cast<std::uint64_t>(sid) * _impl->segment_cachelines + cl;
            auto&         cslot = _impl->controls[hdr->address];
            std::uint64_t cur   = cslot.load(std::memory_order_acquire);
            while (true)
            {
               ControlWord cw{cur};
               if (cw.cacheline_offset() != expected_cl_off || !cw.pinned())
                  break;
               std::uint64_t unpinned = cur & ~(ControlWord::kPinnedBit | ControlWord::kReadBit);
               if (cslot.compare_exchange_weak(cur, unpinned, std::memory_order_acq_rel,
                                               std::memory_order_acquire))
               {
                  break;
               }
            }
            cl += obj_cl;
         }

         // Retire hot buffer after unpinning
         for (auto it = _impl->owned_hot_buffers.begin(); it != _impl->owned_hot_buffers.end();
              ++it)
         {
            if (it->get() == hot_ptr)
            {
               _impl->retired_hot_buffers.push_back(std::move(*it));
               _impl->owned_hot_buffers.erase(it);
               break;
            }
         }
         ++demoted;
      }
      return demoted;
   }

   std::size_t SegmentAllocator::prune_tail()
   {
      std::lock_guard lk{_impl->alloc_mutex};
      std::size_t     pruned = 0;

      while (!_impl->cold_segment_order.empty())
      {
         std::uint32_t tail_sid = _impl->cold_segment_order.back();
         auto&         seg      = _impl->segments[tail_sid];
         if (seg.live_cachelines.load(std::memory_order_acquire) != 0)
            break;

         std::uint64_t new_len = seg.cold_file_offset.load(std::memory_order_relaxed);
         if (::ftruncate(_impl->cold_fd, static_cast<off_t>(new_len)) != 0)
            throw std::runtime_error("SAL prune_tail ftruncate failed");
         _impl->cold_file_bytes.store(new_len, std::memory_order_release);
         seg.in_use.store(false, std::memory_order_release);
         seg.is_cold.store(false, std::memory_order_release);
         _impl->cold_segment_order.pop_back();
         ++pruned;
      }
      return pruned;
   }

   void SegmentAllocator::set_fiber_yield_hook(FiberYieldHook hook)
   {
      std::unique_lock hlk{_impl->hook_mutex};
      _impl->yield_hook = std::move(hook);
   }

   void SegmentAllocator::set_mfu_difficulty(std::uint32_t difficulty) noexcept
   {
      _impl->mfu_difficulty.store(std::max(1u, difficulty), std::memory_order_relaxed);
   }

   SalStats SegmentAllocator::stats() const
   {
      SalStats s{};
      s.live_objects    = _impl->live_objects.load(std::memory_order_relaxed);
      s.cold_file_bytes = _impl->cold_file_bytes.load(std::memory_order_acquire);
      s.cold_reads      = _impl->cold_reads.load(std::memory_order_relaxed);
      s.mfu_promotions  = _impl->mfu_promotions.load(std::memory_order_relaxed);
      s.fiber_yields    = _impl->fiber_yields.load(std::memory_order_relaxed);
      s.io_uring_active = _impl->cold_reader && _impl->cold_reader->active();
      {
         std::lock_guard qlk{_impl->deferred_mutex};
         s.deferred_queue_depth = _impl->deferred_queue.size();
      }
      {
         std::lock_guard alk{_impl->alloc_mutex};
         for (std::uint32_t i = 0; i < _impl->next_seg_id; ++i)
         {
            if (!_impl->segments[i].in_use.load(std::memory_order_relaxed))
               continue;
            if (_impl->segments[i].is_cold.load(std::memory_order_relaxed))
               ++s.cold_segments;
            else
               ++s.hot_segments;
         }
      }
      return s;
   }
}  // namespace muabase_db
