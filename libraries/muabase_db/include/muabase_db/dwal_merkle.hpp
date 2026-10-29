#pragma once

#include <muabase_db/cow_art.hpp>

#include <array>
#include <cstddef>
#include <cstdint>
#include <filesystem>
#include <memory>
#include <optional>
#include <span>
#include <unordered_map>
#include <vector>

namespace muabase_db
{
   using Checksum256 = std::array<std::uint8_t, 32>;

   struct DwalConfig
   {
      std::size_t swap_threshold_ops = 100'000;  // Auto-freeze RW -> RO threshold per root
      bool        sync_wal_on_write  = false;    // fdatasync each append when true
   };

   struct DwalRootStats
   {
      std::size_t   rw_entries     = 0;
      std::size_t   ro_entries     = 0;
      std::uint64_t flushed_epochs = 0;
      std::uint64_t wal_replayed   = 0;
   };

   // Dual-Mode Write-Ahead Log + Buffered In-Memory Overlay Engine (PsiTri DWAL pattern).
   // Supports up to 512 independent sharded roots, serving live merged reads across
   // active RW overlay -> frozen RO snapshot -> persistent CowArt base snapshot.
   class DwalEngine
   {
     public:
      static constexpr std::uint16_t kMaxRoots = 512;

      DwalEngine(const std::filesystem::path& wal_dir, CowArt& art, DwalConfig cfg = {});
      ~DwalEngine();

      DwalEngine(const DwalEngine&)            = delete;
      DwalEngine& operator=(const DwalEngine&) = delete;

      void    set_base_root(std::uint16_t root_id, ArtRoot base);
      ArtRoot base_root(std::uint16_t root_id) const;

      // Sub-microsecond buffered mutations appended to CRC-framed sequential WAL + RW overlay
      void upsert(std::uint16_t              root_id,
                  std::span<const std::byte> key,
                  std::span<const std::byte> value,
                  ValueTag                   tag = ValueTag::Bytes);

      void remove(std::uint16_t root_id, std::span<const std::byte> key);

      // Freeze active RW overlay into immutable RO snapshot without blocking new RW writes
      bool swap_rw_to_ro(std::uint16_t root_id);

      // Drain frozen RO (and RW if freeze_rw == true) into persistent CowArt in one amortized batch
      ArtRoot flush(std::uint16_t root_id, bool freeze_rw = true);
      void    flush_all();

      // Live merged point & ordered cursor reads across RW -> RO -> CowArt (honoring tombstones)
      std::optional<std::vector<std::byte>> get(std::uint16_t              root_id,
                                                std::span<const std::byte> key,
                                                ValueTag*                  out_tag = nullptr) const;

      std::optional<KvEntry> lower_bound(std::uint16_t              root_id,
                                         std::span<const std::byte> key,
                                         std::size_t                prefix_len = 0) const;

      std::optional<KvEntry> less_than(std::uint16_t              root_id,
                                       std::span<const std::byte> key,
                                       std::size_t                prefix_len = 0) const;

      std::optional<KvEntry> max_with_prefix(std::uint16_t              root_id,
                                             std::span<const std::byte> prefix) const;

      DwalRootStats root_stats(std::uint16_t root_id) const;
      std::filesystem::path wal_file_path() const;

     private:
      struct Impl;
      std::unique_ptr<Impl> _impl;
   };

   struct MerkleSibling
   {
      std::uint8_t           branch_byte = 0;
      bool                   is_leaf     = false;
      std::vector<std::byte> edge_prefix = {};
      Checksum256            core_hash{};
   };

   struct MerkleProofStep
   {
      std::vector<std::byte>     edge_prefix = {};
      bool                       has_terminal = false;
      Checksum256                terminal_hash{};  // Populated when terminal is a sibling witness
      std::vector<MerkleSibling> siblings = {};    // Sorted by branch_byte
   };

   struct MerkleProof
   {
      bool                         is_inclusion = false;
      bool                         empty_tree   = false;
      bool                         single_entry = false;
      std::vector<std::byte>       target_key   = {};
      std::vector<std::byte>       root_prefix  = {};
      // Used for single-entry non-inclusion witness (proves the sole key != query_key without
      // revealing its value payload) or leaf-mismatch witness at the bottom of a path:
      std::vector<std::byte>       witness_key       = {};
      Checksum256                  witness_core_hash{};
      std::vector<MerkleProofStep> steps             = {};
   };

   struct MerkleStats
   {
      std::uint64_t nodes_hashed = 0;
      std::uint64_t cache_hits   = 0;
   };

   // QMDB / NOMT-inspired Batch-Boundary Canonical Patricia-Radix SHA-256 Merkleizer.
   // Computes canonical insertion-order-independent state roots at the DWAL -> CowArt
   // batch boundary while memoizing unmodified SAL PtrAddress subtrees across blocks.
   class Merkleizer
   {
     public:
      explicit Merkleizer(CowArt& art);

      Checksum256 compute_root(const ArtRoot& root);

      MerkleProof prove_inclusion(const ArtRoot& root, std::span<const std::byte> key);
      MerkleProof prove_non_inclusion(const ArtRoot& root, std::span<const std::byte> key);

      static Checksum256 hash_entry(std::span<const std::byte> key,
                                    ValueTag                   tag,
                                    std::span<const std::byte> value);

      static bool verify_inclusion(const Checksum256&         expected_root,
                                   std::span<const std::byte> key,
                                   std::span<const std::byte> value,
                                   const MerkleProof&         proof,
                                   ValueTag                   tag = ValueTag::Bytes);

      static bool verify_non_inclusion(const Checksum256&         expected_root,
                                       std::span<const std::byte> key,
                                       const MerkleProof&         proof);

      MerkleStats stats() const noexcept { return _stats; }
      void        clear_cache() noexcept;

     private:
      struct CachedSummary
      {
         std::uint64_t          count = 0;
         std::vector<std::byte> edge_prefix;
         Checksum256            core_hash{};
      };

      CowArt&                                       _art;
      std::unordered_map<std::uint64_t, CachedSummary> _memo;
      MerkleStats                                   _stats{};
   };
}  // namespace muabase_db
