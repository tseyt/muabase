#pragma once

#include <muabase_db/dwal_merkle.hpp>

#include <cstddef>
#include <cstdint>
#include <memory>
#include <optional>
#include <span>
#include <string_view>
#include <vector>

namespace muabase_db
{
   struct AccessRange
   {
      std::vector<std::byte> lower    = {};
      std::vector<std::byte> upper    = {};  // Empty denotes +infinity
      bool                   is_write = false;

      bool operator==(const AccessRange&) const noexcept = default;
   };

   // Read/Write interval tracker matching psibase::DbChangeSet semantics.
   class ChangeSet
   {
     public:
      void on_read(std::span<const std::byte> key);
      void on_write(std::span<const std::byte> key);
      void on_greater_equal(std::span<const std::byte> key,
                            std::size_t                prefix_len,
                            bool                       found,
                            std::span<const std::byte> result_key);
      void on_less_than(std::span<const std::byte> key,
                        std::size_t                prefix_len,
                        bool                       found,
                        std::span<const std::byte> result_key);
      void on_max(std::span<const std::byte> prefix,
                  bool                       found,
                  std::span<const std::byte> result_key);

      void        normalize();
      std::size_t size() const noexcept { return _ranges.size(); }
      void        truncate(std::size_t count);

      const std::vector<AccessRange>& ranges() const noexcept { return _ranges; }
      void                            append_all(const ChangeSet& other);

      bool operator==(const ChangeSet&) const noexcept = default;

     private:
      std::vector<AccessRange> _ranges;
   };

   struct CommittedTx
   {
      ArtRoot   base_root;
      ArtRoot   post_root;
      ChangeSet changes;
   };

   class TxSession;

   // Live transactional handle bound to a named subtree inside a TxSession.
   // Fixes PsiTri's detached-subtree hazard (psitri-subtree-tx-tracking): all mutations
   // automatically propagate through nested savepoint frames and materialize on commit().
   class SubtreeHandle
   {
     public:
      SubtreeHandle(TxSession& session, std::vector<std::byte> subtree_key);

      std::optional<std::vector<std::byte>> get(std::span<const std::byte> key);
      void                                  upsert(std::span<const std::byte> key,
                                                   std::span<const std::byte> value);
      bool                                  remove(std::span<const std::byte> key);

      std::optional<KvEntry> lower_bound(std::span<const std::byte> key,
                                         std::size_t                prefix_len = 0);
      std::optional<KvEntry> less_than(std::span<const std::byte> key,
                                       std::size_t                prefix_len = 0);
      std::optional<KvEntry> max_with_prefix(std::span<const std::byte> prefix);

      std::uint64_t count() const;

     private:
      TxSession*             _session = nullptr;
      std::vector<std::byte> _subtree_key;
   };

   class TxSession
   {
     public:
      TxSession(CowArt& art, ArtRoot base_root);
      ~TxSession();

      TxSession(const TxSession&)            = delete;
      TxSession& operator=(const TxSession&) = delete;

      // Top-level KV operations
      std::optional<std::vector<std::byte>> get(std::span<const std::byte> key);
      void                                  upsert(std::span<const std::byte> key,
                                                   std::span<const std::byte> value);
      bool                                  remove(std::span<const std::byte> key);

      std::optional<KvEntry> lower_bound(std::span<const std::byte> key,
                                         std::size_t                prefix_len = 0);
      std::optional<KvEntry> less_than(std::span<const std::byte> key,
                                       std::size_t                prefix_len = 0);
      std::optional<KvEntry> max_with_prefix(std::span<const std::byte> prefix);

      // Tracked hierarchical service/table subtree access
      SubtreeHandle open_subtree(std::span<const std::byte> subtree_key);

      // Nested Wasm action savepoint frames
      std::size_t frame_depth() const noexcept;
      void        push_frame();
      void        commit_frame();
      void        abort_frame();

      // Materializes all dirty subtrees into the root and returns a sealable CommittedTx
      CommittedTx commit();

     private:
      friend class SubtreeHandle;
      struct Impl;
      std::unique_ptr<Impl> _impl;
   };

   enum class ConflictKind : std::uint8_t
   {
      ReadWrite  = 1,
      WriteWrite = 2,
   };

   // Exact conflicting key interval (Tahrir Mahall al-Niza')
   struct ConflictInfo
   {
      ConflictKind           kind        = ConflictKind::WriteWrite;
      std::vector<std::byte> locus_lower = {};
      std::vector<std::byte> locus_upper = {};  // Empty denotes +infinity

      bool operator==(const ConflictInfo&) const noexcept = default;
   };

   struct MergeResult
   {
      bool                        ok = false;
      CommittedTx                 merged;
      Checksum256                 merkle_root{};
      std::optional<ConflictInfo> conflict = std::nullopt;
   };

   // Commutative 3-way algebraic merge (R_merged = tx_a (+)_(base_root) tx_b).
   // Allows shared Read-Read intervals and disjoint writes (including disjoint keys within
   // the same service subtree!), while deterministically isolating Read-Write and Write-Write
   // conflicts to their exact key locus.
   MergeResult merge_transactions(CowArt&            art,
                                  const ArtRoot&     base_root,
                                  const CommittedTx& tx_a,
                                  const CommittedTx& tx_b,
                                  Merkleizer*        merkle = nullptr);
}  // namespace muabase_db
