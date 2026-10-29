#pragma once

#include <muabase_db/sal.hpp>

#include <cstddef>
#include <cstdint>
#include <optional>
#include <span>
#include <string_view>
#include <utility>
#include <vector>

namespace muabase_db
{
   enum class ValueTag : std::uint8_t
   {
      Bytes   = 0,
      Subtree = 1,
   };

   struct KvEntry
   {
      std::vector<std::byte> key;
      std::vector<std::byte> value;
      ValueTag               tag = ValueTag::Bytes;
   };

   // Persistent Copy-on-Write snapshot handle backed by SegmentAllocator.
   // Copying retains the root in O(1); destruction enqueues zero-ref nodes to SAL's
   // flat deferred-release queue without recursive stack frames.
   class ArtRoot
   {
     public:
      ArtRoot() noexcept = default;
      ArtRoot(SegmentAllocator* sal, PtrAddress addr, bool adopt_existing_ref = true) noexcept;
      ArtRoot(const ArtRoot& other);
      ArtRoot(ArtRoot&& other) noexcept;
      ArtRoot& operator=(const ArtRoot& other);
      ArtRoot& operator=(ArtRoot&& other) noexcept;
      ~ArtRoot();

      explicit operator bool() const noexcept { return static_cast<bool>(_addr); }
      PtrAddress        address() const noexcept { return _addr; }
      SegmentAllocator* allocator() const noexcept { return _sal; }
      void              reset() noexcept;

     private:
      SegmentAllocator* _sal = nullptr;
      PtrAddress        _addr{};
   };

   class CowArt
   {
     public:
      static constexpr std::size_t kMaxLeafEntries = 16;
      static constexpr std::size_t kMaxLeafBytes   = 2048;

      explicit CowArt(SegmentAllocator& sal) noexcept;

      SegmentAllocator& allocator() noexcept { return _sal; }
      const SegmentAllocator& allocator() const noexcept { return _sal; }

      // Child enumerator suitable for SegmentAllocator::drain_deferred_releases().
      static void enumerate_node_children(ObjectKind                             kind,
                                          std::span<const std::byte>             payload,
                                          const std::function<void(PtrAddress)>& emit);

      std::size_t drain_garbage(std::size_t max_steps = 4096);

      // Point lookup (8-bit raw byte dispatch, zero 6-bit key inflation)
      std::optional<std::vector<std::byte>> get(const ArtRoot&             root,
                                                std::span<const std::byte> key,
                                                ValueTag*                  out_tag = nullptr) const;

      // Zero-copy point read on hot resident leaves/nodes
      template <typename Fn>
      bool view(const ArtRoot& root, std::span<const std::byte> key, Fn&& fn) const
      {
         bool found = false;
         visit_value(root, key,
                     [&](ValueTag tag, std::span<const std::byte> val)
                     {
                        found = true;
                        std::forward<Fn>(fn)(tag, val);
                     });
         return found;
      }

      // Persistent COW mutations (input root remains immutable)
      ArtRoot upsert(const ArtRoot&             root,
                     std::span<const std::byte> key,
                     std::span<const std::byte> value,
                     ValueTag                   tag = ValueTag::Bytes);

      ArtRoot upsert_subtree(const ArtRoot&             root,
                             std::span<const std::byte> key,
                             const ArtRoot&             child_root);

      std::optional<ArtRoot> get_subtree(const ArtRoot&             root,
                                         std::span<const std::byte> key) const;

      std::pair<ArtRoot, bool> remove(const ArtRoot& root, std::span<const std::byte> key);

      // Ordered cursor operations matching psibase native KV host API:
      //   lower_bound      -> kvGreaterEqual(key, prefix_len)
      //   less_than        -> kvLessThan(key, prefix_len)
      //   max_with_prefix  -> kvMax(prefix)
      std::optional<KvEntry> lower_bound(const ArtRoot&             root,
                                         std::span<const std::byte> key,
                                         std::size_t                prefix_len = 0) const;

      std::optional<KvEntry> less_than(const ArtRoot&             root,
                                       std::span<const std::byte> key,
                                       std::size_t                prefix_len = 0) const;

      std::optional<KvEntry> max_with_prefix(const ArtRoot&             root,
                                             std::span<const std::byte> prefix) const;

      // O(1) total count and O(log N) range count / range removal over [lower, upper)
      // (empty upper denotes +infinity).
      std::uint64_t count(const ArtRoot& root) const;

      std::uint64_t count_range(const ArtRoot&             root,
                                std::span<const std::byte> lower,
                                std::span<const std::byte> upper) const;

      ArtRoot remove_range(const ArtRoot&             root,
                           std::span<const std::byte> lower,
                           std::span<const std::byte> upper);

     private:
      void visit_value(
          const ArtRoot&                                                       root,
          std::span<const std::byte>                                           key,
          const std::function<void(ValueTag, std::span<const std::byte>)>& cb) const;

      SegmentAllocator& _sal;
   };
}  // namespace muabase_db
