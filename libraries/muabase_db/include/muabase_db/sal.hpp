#pragma once

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <deque>
#include <filesystem>
#include <functional>
#include <memory>
#include <mutex>
#include <optional>
#include <shared_mutex>
#include <span>
#include <vector>

namespace muabase_db
{
   inline constexpr std::size_t   kCachelineBytes    = 64;
   inline constexpr std::size_t   kSiblingGroupSlots = 16;
   inline constexpr std::uint32_t kMaxRefCount       = (1u << 21) - 1;  // 2,097,151
   inline constexpr std::uint64_t kMaxCachelineIndex = (1ull << 41) - 1;

   enum class ObjectKind : std::uint8_t
   {
      RawBytes  = 0,
      ArtInner  = 1,
      BTreeLeaf = 2,
      Subtree   = 3,
      MerkleExt = 4,
   };

   struct PtrAddress
   {
      std::uint32_t value = 0;

      constexpr explicit operator bool() const noexcept { return value != 0; }
      constexpr auto     operator<=>(const PtrAddress&) const noexcept = default;
   };

   // 8-byte atomic ControlBlock entry:
   //   bits [0..20]  (21b): reference count (0 = free, up to 2,097,151)
   //   bits [21..61] (41b): 64-byte cacheline offset across segment address space (128 TiB)
   //   bit  [62]     ( 1b): pinned/resident in RAM (1 = ~2ns direct deref, 0 = cold async read)
   //   bit  [63]     ( 1b): MFU read mark bit (scan-resistant 2nd-touch promotion)
   struct ControlWord
   {
      static constexpr std::uint64_t kRefMask     = (1ull << 21) - 1ull;
      static constexpr unsigned      kOffsetShift = 21;
      static constexpr std::uint64_t kOffsetMask  = ((1ull << 41) - 1ull) << kOffsetShift;
      static constexpr std::uint64_t kPinnedBit   = 1ull << 62;
      static constexpr std::uint64_t kReadBit     = 1ull << 63;

      std::uint64_t raw = 0;

      constexpr std::uint32_t ref_count() const noexcept
      {
         return static_cast<std::uint32_t>(raw & kRefMask);
      }
      constexpr std::uint64_t cacheline_offset() const noexcept
      {
         return (raw & kOffsetMask) >> kOffsetShift;
      }
      constexpr bool pinned() const noexcept { return (raw & kPinnedBit) != 0; }
      constexpr bool read_marked() const noexcept { return (raw & kReadBit) != 0; }

      static constexpr ControlWord pack(std::uint32_t ref,
                                        std::uint64_t cacheline_off,
                                        bool          is_pinned,
                                        bool          is_read) noexcept
      {
         return ControlWord{(static_cast<std::uint64_t>(ref) & kRefMask) |
                            ((cacheline_off << kOffsetShift) & kOffsetMask) |
                            (is_pinned ? kPinnedBit : 0ull) | (is_read ? kReadBit : 0ull)};
      }
   };
   static_assert(sizeof(ControlWord) == 8);
   static_assert(sizeof(std::atomic<std::uint64_t>) == 8);

   struct alignas(16) AllocHeader
   {
      static constexpr std::uint32_t kMagic24 = 0x4D5541u;  // "MUA"

      std::uint32_t magic_and_kind = 0;
      std::uint32_t payload_size   = 0;
      std::uint32_t address        = 0;
      std::uint32_t flags          = 0;

      static constexpr std::uint32_t pack_magic(ObjectKind k) noexcept
      {
         return (kMagic24 << 8) | static_cast<std::uint32_t>(k);
      }
      constexpr bool       valid() const noexcept { return (magic_and_kind >> 8) == kMagic24; }
      constexpr ObjectKind kind() const noexcept
      {
         return static_cast<ObjectKind>(magic_and_kind & 0xFFu);
      }
   };
   static_assert(sizeof(AllocHeader) == 16);

   struct SalConfig
   {
      std::size_t   segment_bytes       = 32ull * 1024 * 1024;  // 32 MiB default (tunable in tests)
      std::size_t   initial_control_cap = 1024;
      std::uint32_t mfu_difficulty  = 1;  // Promotion probability = 1 / (difficulty^2) after mark
      bool          enable_io_uring = true;
   };

   struct SalStats
   {
      std::size_t   live_objects         = 0;
      std::size_t   hot_segments         = 0;
      std::size_t   cold_segments        = 0;
      std::uint64_t cold_file_bytes      = 0;
      std::uint64_t cold_reads           = 0;
      std::uint64_t mfu_promotions       = 0;
      std::uint64_t fiber_yields         = 0;
      std::size_t   deferred_queue_depth = 0;
      bool          io_uring_active      = false;
   };

   using ChildEnumeratorFn = std::function<
       void(ObjectKind, std::span<const std::byte>, const std::function<void(PtrAddress)>&)>;
   using FiberYieldHook = std::function<void()>;

   class SegmentAllocator
   {
     public:
      SegmentAllocator(const std::filesystem::path& dir, SalConfig cfg = {});
      ~SegmentAllocator();

      SegmentAllocator(const SegmentAllocator&)            = delete;
      SegmentAllocator& operator=(const SegmentAllocator&) = delete;

      // Allocate a 64B-aligned object in the active hot segment.
      // When locality_hint is set, co-locates the new ControlBlock in the same 16-slot
      // (128-byte / 2-cacheline) group whenever a free slot exists in that group.
      PtrAddress allocate(std::span<const std::byte> payload,
                          ObjectKind                 kind          = ObjectKind::RawBytes,
                          PtrAddress                 locality_hint = {});

      // Atomic reference-count operations (up to kMaxRefCount = 2,097,151).
      std::uint32_t retain(PtrAddress addr, std::uint32_t delta = 1);
      std::uint32_t ref_count(PtrAddress addr) const noexcept;
      ControlWord   inspect_control(PtrAddress addr) const noexcept;

      // O(1) non-recursive release: when ref_count reaches 0, enqueues addr onto the flat
      // deferred-release queue rather than recursing on the call stack (fixing Triedent#519).
      bool release(PtrAddress addr);

      // Iteratively drains up to max_steps zero-ref objects from the deferred queue in O(1) stack.
      std::size_t drain_deferred_releases(std::size_t              max_steps        = 1024,
                                          const ChildEnumeratorFn& child_enumerator = {});

      // Zero-copy read on pinned hot segments (~2ns); swizzled async cold read (Linux io_uring
      // with pread fallback + fiber yield hook + scan-resistant 1/N^2 MFU promotion) when cold.
      template <typename Fn>
      decltype(auto) read(PtrAddress addr, Fn&& visitor)
      {
         std::optional<std::vector<std::byte>> cold_buf;
         ObjectKind                            kind = ObjectKind::RawBytes;
         std::span<const std::byte>            view = acquire_view(addr, kind, cold_buf);
         return std::forward<Fn>(visitor)(kind, view);
      }

      // Demote a sealed hot segment (or all non-active hot segments) to the cold segment file,
      // clearing the pinned bit atomically on live ControlBlocks and freeing hot RAM.
      std::size_t demote_sealed_hot_segments();

      // Truncate trailing empty segments from the tail of the cold segment log.
      std::size_t prune_tail();

      void set_fiber_yield_hook(FiberYieldHook hook);
      void set_mfu_difficulty(std::uint32_t difficulty) noexcept;

      SalStats stats() const;

      static constexpr std::size_t sibling_group(PtrAddress a) noexcept { return a.value / 16u; }

     private:
      std::span<const std::byte> acquire_view(PtrAddress                             addr,
                                              ObjectKind&                            out_kind,
                                              std::optional<std::vector<std::byte>>& cold_buf);

      struct Impl;
      std::unique_ptr<Impl> _impl;
   };
}  // namespace muabase_db
