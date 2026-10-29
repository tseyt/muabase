#include <muabase_db/tx_session.hpp>

#include <algorithm>
#include <cstring>
#include <map>
#include <set>
#include <stdexcept>

namespace muabase_db
{
   namespace
   {
      int lex_compare(std::span<const std::byte> a, std::span<const std::byte> b) noexcept
      {
         const std::size_t n = std::min(a.size(), b.size());
         if (n > 0)
         {
            int c = std::memcmp(a.data(), b.data(), n);
            if (c != 0)
               return c;
         }
         if (a.size() < b.size())
            return -1;
         if (a.size() > b.size())
            return 1;
         return 0;
      }

      bool starts_with(std::span<const std::byte> full, std::span<const std::byte> prefix) noexcept
      {
         if (full.size() < prefix.size())
            return false;
         return prefix.empty() || std::memcmp(full.data(), prefix.data(), prefix.size()) == 0;
      }

      struct ByteLess
      {
         using is_transparent = void;

         bool operator()(const std::vector<std::byte>& a,
                         const std::vector<std::byte>& b) const noexcept
         {
            return lex_compare(a, b) < 0;
         }
         bool operator()(const std::vector<std::byte>& a,
                         std::span<const std::byte>    b) const noexcept
         {
            return lex_compare(a, b) < 0;
         }
         bool operator()(std::span<const std::byte>    a,
                         const std::vector<std::byte>& b) const noexcept
         {
            return lex_compare(a, b) < 0;
         }
      };

      std::vector<std::byte> key_exact(std::span<const std::byte> k)
      {
         return {k.begin(), k.end()};
      }

      std::vector<std::byte> key_next(std::span<const std::byte> k)
      {
         std::vector<std::byte> out;
         out.reserve(k.size() + 1);
         out.insert(out.end(), k.begin(), k.end());
         out.push_back(std::byte{0});
         return out;
      }

      std::vector<std::byte> key_next_prefix(std::span<const std::byte> pfx)
      {
         std::vector<std::byte> out(pfx.begin(), pfx.end());
         while (!out.empty() && std::to_integer<std::uint8_t>(out.back()) == 0xFFu)
         {
            out.pop_back();
         }
         if (!out.empty())
         {
            auto next = static_cast<std::uint8_t>(std::to_integer<std::uint8_t>(out.back()) + 1u);
            out.back() = static_cast<std::byte>(next);
         }
         return out;
      }

      // Compare two upper bounds where empty denotes +infinity
      int upper_compare(std::span<const std::byte> u1, std::span<const std::byte> u2) noexcept
      {
         if (u1.empty() && u2.empty())
            return 0;
         if (u1.empty())
            return 1;
         if (u2.empty())
            return -1;
         return lex_compare(u1, u2);
      }

      // Check whether key `lower` <= upper bound `u` (empty `u` = +infinity)
      bool lower_le_upper(std::span<const std::byte> lower, std::span<const std::byte> u) noexcept
      {
         if (u.empty())
            return true;
         return lex_compare(lower, u) <= 0;
      }

      // Check whether key `lower` < upper bound `u` (empty `u` = +infinity)
      bool lower_lt_upper(std::span<const std::byte> lower, std::span<const std::byte> u) noexcept
      {
         if (u.empty())
            return true;
         return lex_compare(lower, u) < 0;
      }

      // Domain-scoped key encodings for ChangeSet interval tracking:
      //   Top-level keys:  0x00 || key                  (namespace prefix = [0x00], len 1)
      //   Subtree keys:    0x01 || u16_be(|S|) || S || inner_key
      //                    (namespace prefix = [0x01 || u16_be(|S|) || S], len 3 + |S|)
      std::vector<std::byte> encode_top_key(std::span<const std::byte> k)
      {
         std::vector<std::byte> out;
         out.reserve(1 + k.size());
         out.push_back(std::byte{0x00});
         out.insert(out.end(), k.begin(), k.end());
         return out;
      }

      std::vector<std::byte> encode_subtree_ns(std::span<const std::byte> sub_key)
      {
         if (sub_key.size() > 0xFFFFu)
            throw std::invalid_argument("Subtree key exceeds 65535 bytes");
         const auto             slen = static_cast<std::uint16_t>(sub_key.size());
         std::vector<std::byte> out;
         out.reserve(3 + sub_key.size());
         out.push_back(std::byte{0x01});
         out.push_back(static_cast<std::byte>((slen >> 8) & 0xFFu));
         out.push_back(static_cast<std::byte>(slen & 0xFFu));
         out.insert(out.end(), sub_key.begin(), sub_key.end());
         return out;
      }

      std::vector<std::byte> encode_subtree_key(std::span<const std::byte> sub_key,
                                                std::span<const std::byte> inner_key)
      {
         auto out = encode_subtree_ns(sub_key);
         out.insert(out.end(), inner_key.begin(), inner_key.end());
         return out;
      }

      bool in_raw_range(std::span<const std::byte> key,
                        std::span<const std::byte> lower,
                        std::span<const std::byte> upper) noexcept
      {
         if (lex_compare(key, lower) < 0)
            return false;
         if (!upper.empty() && lex_compare(key, upper) >= 0)
            return false;
         return true;
      }

      // Splice [raw_lower, raw_upper) from `src_root` onto `dst_root` (Bytes entries only when
      // `bytes_only == true` so top-level KV splices never clobber Subtree entries).
      ArtRoot splice_raw_range(CowArt&                    art,
                               ArtRoot                    dst_root,
                               const ArtRoot&             src_root,
                               std::span<const std::byte> raw_lower,
                               std::span<const std::byte> raw_upper,
                               bool                       bytes_only)
      {
         if (!bytes_only)
         {
            dst_root = art.remove_range(dst_root, raw_lower, raw_upper);
         }
         else
         {
            std::vector<std::byte> probe(raw_lower.begin(), raw_lower.end());
            while (true)
            {
               auto cand = art.lower_bound(dst_root, probe);
               if (!cand.has_value() || !in_raw_range(cand->key, raw_lower, raw_upper))
                  break;
               auto next_probe = key_next(cand->key);
               if (cand->tag == ValueTag::Bytes)
                  dst_root = art.remove(dst_root, cand->key).first;
               probe = std::move(next_probe);
            }
         }

         std::vector<std::byte> probe(raw_lower.begin(), raw_lower.end());
         while (true)
         {
            auto cand = art.lower_bound(src_root, probe);
            if (!cand.has_value() || !in_raw_range(cand->key, raw_lower, raw_upper))
               break;
            auto next_probe = key_next(cand->key);
            if (!bytes_only || cand->tag == ValueTag::Bytes)
               dst_root = art.upsert(dst_root, cand->key, cand->value, cand->tag);
            probe = std::move(next_probe);
         }
         return dst_root;
      }
   }  // namespace

   // =========================================================================
   // ChangeSet
   // =========================================================================

   void ChangeSet::on_read(std::span<const std::byte> key)
   {
      _ranges.push_back(AccessRange{
          .lower    = key_exact(key),
          .upper    = key_next(key),
          .is_write = false,
      });
   }

   void ChangeSet::on_write(std::span<const std::byte> key)
   {
      _ranges.push_back(AccessRange{
          .lower    = key_exact(key),
          .upper    = key_next(key),
          .is_write = true,
      });
   }

   void ChangeSet::on_greater_equal(std::span<const std::byte> key,
                                    std::size_t                prefix_len,
                                    bool                       found,
                                    std::span<const std::byte> result_key)
   {
      const std::size_t clamped = std::min(prefix_len, key.size());
      _ranges.push_back(AccessRange{
          .lower    = key_exact(key),
          .upper    = found ? key_next(result_key) : key_next_prefix(key.subspan(0, clamped)),
          .is_write = false,
      });
   }

   void ChangeSet::on_less_than(std::span<const std::byte> key,
                                std::size_t                prefix_len,
                                bool                       found,
                                std::span<const std::byte> result_key)
   {
      const std::size_t clamped = std::min(prefix_len, key.size());
      _ranges.push_back(AccessRange{
          .lower    = found ? key_exact(result_key) : key_exact(key.subspan(0, clamped)),
          .upper    = key_exact(key),
          .is_write = false,
      });
   }

   void ChangeSet::on_max(std::span<const std::byte> prefix,
                          bool                       found,
                          std::span<const std::byte> result_key)
   {
      _ranges.push_back(AccessRange{
          .lower    = found ? key_exact(result_key) : key_exact(prefix),
          .upper    = key_next_prefix(prefix),
          .is_write = false,
      });
   }

   void ChangeSet::normalize()
   {
      if (_ranges.empty())
         return;

      // Sort by (lower, is_write, upper) and coalesce ONLY ranges with the same `is_write`
      // mode, or keep read vs write fidelity? Wait: if a transaction both reads AND writes
      // key K, keeping write=true on K is exact; HOWEVER, coalescing an adjacent READ scan
      // [order/15, order/30\x00) with a WRITE [order/30\x00, ...) must NOT promote the read scan
      // into a write range (otherwise splicing would delete unread keys in the read scan!).
      // Therefore we coalesce overlapping/adjacent ranges ONLY when `out->is_write == pos->is_write`!
      std::sort(_ranges.begin(), _ranges.end(),
                [](const AccessRange& a, const AccessRange& b)
                {
                   if (a.is_write != b.is_write)
                      return !a.is_write && b.is_write;  // reads first, writes second
                   int c = lex_compare(a.lower, b.lower);
                   if (c != 0)
                      return c < 0;
                   return upper_compare(a.upper, b.upper) < 0;
                });

      std::vector<AccessRange> compacted;
      compacted.reserve(_ranges.size());
      for (auto& r : _ranges)
      {
         // Drop empty intervals [x, x)
         if (!r.upper.empty() && lex_compare(r.lower, r.upper) >= 0)
            continue;

         if (!compacted.empty() && compacted.back().is_write == r.is_write &&
             lower_le_upper(r.lower, compacted.back().upper))
         {
            if (upper_compare(compacted.back().upper, r.upper) < 0)
               compacted.back().upper = std::move(r.upper);
         }
         else
         {
            compacted.push_back(std::move(r));
         }
      }
      _ranges = std::move(compacted);
   }

   void ChangeSet::truncate(std::size_t count)
   {
      if (count < _ranges.size())
         _ranges.resize(count);
   }

   void ChangeSet::append_all(const ChangeSet& other)
   {
      _ranges.insert(_ranges.end(), other._ranges.begin(), other._ranges.end());
   }

   // =========================================================================
   // TxSession + SubtreeHandle
   // =========================================================================

   struct TxSession::Impl
   {
      struct Frame
      {
         ArtRoot                                             top_root;
         std::map<std::vector<std::byte>, ArtRoot, ByteLess> open_subtrees       = {};
         std::set<std::vector<std::byte>, ByteLess>          dirty_subtrees      = {};
         std::size_t                                         changeset_watermark = 0;
      };

      CowArt&            art;
      ArtRoot            initial_base;
      std::vector<Frame> frames;
      ChangeSet          changes;

      Impl(CowArt& a, ArtRoot base) : art(a), initial_base(base)
      {
         frames.push_back(Frame{.top_root = std::move(base), .changeset_watermark = 0});
      }

      Frame&       cur() { return frames.back(); }
      const Frame& cur() const { return frames.back(); }

      ArtRoot& ensure_subtree(std::span<const std::byte> sub_key)
      {
         auto& f = cur();
         if (auto it = f.open_subtrees.find(sub_key); it != f.open_subtrees.end())
            return it->second;

         ArtRoot loaded = art.get_subtree(f.top_root, sub_key).value_or(ArtRoot{});
         auto [ins, _]  = f.open_subtrees.emplace(key_exact(sub_key), std::move(loaded));
         return ins->second;
      }

      const ArtRoot* peek_or_load_subtree(std::span<const std::byte> sub_key,
                                          std::optional<ArtRoot>&    scratch) const
      {
         const auto& f = cur();
         if (auto it = f.open_subtrees.find(sub_key); it != f.open_subtrees.end())
            return &it->second;
         scratch = art.get_subtree(f.top_root, sub_key).value_or(ArtRoot{});
         return &(*scratch);
      }
   };

   SubtreeHandle::SubtreeHandle(TxSession& session, std::vector<std::byte> subtree_key)
       : _session(&session), _subtree_key(std::move(subtree_key))
   {
   }

   std::optional<std::vector<std::byte>> SubtreeHandle::get(std::span<const std::byte> key)
   {
      auto& sub = _session->_impl->ensure_subtree(_subtree_key);
      _session->_impl->changes.on_read(encode_subtree_key(_subtree_key, key));
      return _session->_impl->art.get(sub, key);
   }

   void SubtreeHandle::upsert(std::span<const std::byte> key, std::span<const std::byte> value)
   {
      auto& f   = _session->_impl->cur();
      auto& sub = _session->_impl->ensure_subtree(_subtree_key);
      sub       = _session->_impl->art.upsert(sub, key, value);
      f.dirty_subtrees.insert(_subtree_key);
      _session->_impl->changes.on_write(encode_subtree_key(_subtree_key, key));
   }

   bool SubtreeHandle::remove(std::span<const std::byte> key)
   {
      auto& f                = _session->_impl->cur();
      auto& sub              = _session->_impl->ensure_subtree(_subtree_key);
      auto [next_sub, erased] = _session->_impl->art.remove(sub, key);
      sub                     = std::move(next_sub);
      f.dirty_subtrees.insert(_subtree_key);
      _session->_impl->changes.on_write(encode_subtree_key(_subtree_key, key));
      return erased;
   }

   std::optional<KvEntry> SubtreeHandle::lower_bound(std::span<const std::byte> key,
                                                     std::size_t                prefix_len)
   {
      auto& sub         = _session->_impl->ensure_subtree(_subtree_key);
      auto  res         = _session->_impl->art.lower_bound(sub, key, prefix_len);
      auto  scoped_k    = encode_subtree_key(_subtree_key, key);
      auto  ns_len      = 3u + _subtree_key.size();
      auto  scoped_plen = ns_len + std::min(prefix_len, key.size());
      auto  scoped_res  = res.has_value() ? encode_subtree_key(_subtree_key, res->key)
                                          : std::vector<std::byte>{};
      _session->_impl->changes.on_greater_equal(scoped_k, scoped_plen, res.has_value(), scoped_res);
      return res;
   }

   std::optional<KvEntry> SubtreeHandle::less_than(std::span<const std::byte> key,
                                                   std::size_t                prefix_len)
   {
      auto& sub         = _session->_impl->ensure_subtree(_subtree_key);
      auto  res         = _session->_impl->art.less_than(sub, key, prefix_len);
      auto  scoped_k    = encode_subtree_key(_subtree_key, key);
      auto  ns_len      = 3u + _subtree_key.size();
      auto  scoped_plen = ns_len + std::min(prefix_len, key.size());
      auto  scoped_res  = res.has_value() ? encode_subtree_key(_subtree_key, res->key)
                                          : std::vector<std::byte>{};
      _session->_impl->changes.on_less_than(scoped_k, scoped_plen, res.has_value(), scoped_res);
      return res;
   }

   std::optional<KvEntry> SubtreeHandle::max_with_prefix(std::span<const std::byte> prefix)
   {
      auto& sub        = _session->_impl->ensure_subtree(_subtree_key);
      auto  res        = _session->_impl->art.max_with_prefix(sub, prefix);
      auto  scoped_pfx = encode_subtree_key(_subtree_key, prefix);
      auto  scoped_res = res.has_value() ? encode_subtree_key(_subtree_key, res->key)
                                         : std::vector<std::byte>{};
      _session->_impl->changes.on_max(scoped_pfx, res.has_value(), scoped_res);
      return res;
   }

   std::uint64_t SubtreeHandle::count() const
   {
      std::optional<ArtRoot> scratch;
      const ArtRoot*         sub = _session->_impl->peek_or_load_subtree(_subtree_key, scratch);
      return _session->_impl->art.count(*sub);
   }

   TxSession::TxSession(CowArt& art, ArtRoot base_root)
       : _impl(std::make_unique<Impl>(art, std::move(base_root)))
   {
   }

   TxSession::~TxSession() = default;

   std::optional<std::vector<std::byte>> TxSession::get(std::span<const std::byte> key)
   {
      _impl->changes.on_read(encode_top_key(key));
      ValueTag tag = ValueTag::Bytes;
      auto     res = _impl->art.get(_impl->cur().top_root, key, &tag);
      if (!res || tag != ValueTag::Bytes)
         return std::nullopt;
      return res;
   }

   void TxSession::upsert(std::span<const std::byte> key, std::span<const std::byte> value)
   {
      auto& f    = _impl->cur();
      f.top_root = _impl->art.upsert(f.top_root, key, value, ValueTag::Bytes);
      _impl->changes.on_write(encode_top_key(key));
   }

   bool TxSession::remove(std::span<const std::byte> key)
   {
      auto& f                   = _impl->cur();
      auto [next_root, removed] = _impl->art.remove(f.top_root, key);
      f.top_root                = std::move(next_root);
      _impl->changes.on_write(encode_top_key(key));
      return removed;
   }

   std::optional<KvEntry> TxSession::lower_bound(std::span<const std::byte> key,
                                                 std::size_t                prefix_len)
   {
      std::vector<std::byte> probe(key.begin(), key.end());
      std::optional<KvEntry> res;
      while (true)
      {
         auto cand = _impl->art.lower_bound(_impl->cur().top_root, probe, prefix_len);
         if (!cand.has_value() || cand->tag == ValueTag::Bytes)
         {
            res = std::move(cand);
            break;
         }
         probe = key_next(cand->key);
      }
      auto scoped_k    = encode_top_key(key);
      auto scoped_plen = 1u + std::min(prefix_len, key.size());
      auto scoped_res  = res.has_value() ? encode_top_key(res->key) : std::vector<std::byte>{};
      _impl->changes.on_greater_equal(scoped_k, scoped_plen, res.has_value(), scoped_res);
      return res;
   }

   std::optional<KvEntry> TxSession::less_than(std::span<const std::byte> key,
                                               std::size_t                prefix_len)
   {
      std::vector<std::byte> probe(key.begin(), key.end());
      std::optional<KvEntry> res;
      while (true)
      {
         auto cand = _impl->art.less_than(_impl->cur().top_root, probe, prefix_len);
         if (!cand.has_value() || cand->tag == ValueTag::Bytes)
         {
            res = std::move(cand);
            break;
         }
         probe = std::move(cand->key);
      }
      auto scoped_k    = encode_top_key(key);
      auto scoped_plen = 1u + std::min(prefix_len, key.size());
      auto scoped_res  = res.has_value() ? encode_top_key(res->key) : std::vector<std::byte>{};
      _impl->changes.on_less_than(scoped_k, scoped_plen, res.has_value(), scoped_res);
      return res;
   }

   std::optional<KvEntry> TxSession::max_with_prefix(std::span<const std::byte> prefix)
   {
      auto cand = _impl->art.max_with_prefix(_impl->cur().top_root, prefix);
      while (cand.has_value() && cand->tag != ValueTag::Bytes)
      {
         auto prev_probe = std::move(cand->key);
         cand            = _impl->art.less_than(_impl->cur().top_root, prev_probe, 0);
         if (cand.has_value() && !starts_with(cand->key, prefix))
         {
            cand.reset();
            break;
         }
      }
      auto scoped_pfx = encode_top_key(prefix);
      auto scoped_res = cand.has_value() ? encode_top_key(cand->key) : std::vector<std::byte>{};
      _impl->changes.on_max(scoped_pfx, cand.has_value(), scoped_res);
      return cand;
   }

   SubtreeHandle TxSession::open_subtree(std::span<const std::byte> subtree_key)
   {
      _impl->ensure_subtree(subtree_key);
      return SubtreeHandle{*this, key_exact(subtree_key)};
   }

   std::size_t TxSession::frame_depth() const noexcept
   {
      return _impl->frames.size() - 1;
   }

   void TxSession::push_frame()
   {
      Impl::Frame copy         = _impl->cur();
      copy.dirty_subtrees.clear();
      copy.changeset_watermark = _impl->changes.size();
      _impl->frames.push_back(std::move(copy));
   }

   void TxSession::commit_frame()
   {
      if (_impl->frames.size() <= 1)
         throw std::logic_error("Cannot commit_frame() at root transaction depth");

      Impl::Frame child = std::move(_impl->frames.back());
      _impl->frames.pop_back();

      auto& parent         = _impl->cur();
      parent.top_root      = std::move(child.top_root);
      parent.open_subtrees = std::move(child.open_subtrees);
      parent.dirty_subtrees.insert(child.dirty_subtrees.begin(), child.dirty_subtrees.end());
   }

   void TxSession::abort_frame()
   {
      if (_impl->frames.size() <= 1)
         throw std::logic_error("Cannot abort_frame() at root transaction depth");

      const std::size_t watermark = _impl->frames.back().changeset_watermark;
      _impl->frames.pop_back();
      _impl->changes.truncate(watermark);
   }

   CommittedTx TxSession::commit()
   {
      while (_impl->frames.size() > 1)
         commit_frame();

      auto& root_frame = _impl->frames.front();
      for (const auto& sub_key : root_frame.dirty_subtrees)
      {
         auto it = root_frame.open_subtrees.find(sub_key);
         if (it != root_frame.open_subtrees.end())
         {
            root_frame.top_root =
                _impl->art.upsert_subtree(root_frame.top_root, sub_key, it->second);
         }
      }
      root_frame.dirty_subtrees.clear();
      _impl->changes.normalize();

      return CommittedTx{
          .base_root = _impl->initial_base,
          .post_root = root_frame.top_root,
          .changes   = _impl->changes,
      };
   }

   // =========================================================================
   // Commutative 3-Way Merge (merge_transactions)
   // =========================================================================

   MergeResult merge_transactions(CowArt&            art,
                                  const ArtRoot&     base_root,
                                  const CommittedTx& tx_a,
                                  const CommittedTx& tx_b,
                                  Merkleizer*        merkle)
   {
      ChangeSet cs_a = tx_a.changes;
      ChangeSet cs_b = tx_b.changes;
      cs_a.normalize();
      cs_b.normalize();

      // 1. Symmetric conflict detection (Tahrir Mahall al-Niza')
      std::optional<ConflictInfo> earliest_conflict;
      auto                        consider_conflict = [&](ConflictInfo&& cand)
      {
         if (!earliest_conflict.has_value())
         {
            earliest_conflict = std::move(cand);
            return;
         }
         int lc = lex_compare(cand.locus_lower, earliest_conflict->locus_lower);
         if (lc < 0 ||
             (lc == 0 && upper_compare(cand.locus_upper, earliest_conflict->locus_upper) < 0) ||
             (lc == 0 && upper_compare(cand.locus_upper, earliest_conflict->locus_upper) == 0 &&
              static_cast<std::uint8_t>(cand.kind) >
                  static_cast<std::uint8_t>(earliest_conflict->kind)))
         {
            earliest_conflict = std::move(cand);
         }
      };

      for (const auto& ra : cs_a.ranges())
      {
         for (const auto& rb : cs_b.ranges())
         {
            if (!ra.is_write && !rb.is_write)
               continue;

            const auto& max_lower =
                (lex_compare(ra.lower, rb.lower) >= 0) ? ra.lower : rb.lower;
            const auto& min_upper =
                (upper_compare(ra.upper, rb.upper) <= 0) ? ra.upper : rb.upper;

            if (lower_lt_upper(max_lower, min_upper))
            {
               consider_conflict(ConflictInfo{
                   .kind = (ra.is_write && rb.is_write) ? ConflictKind::WriteWrite
                                                        : ConflictKind::ReadWrite,
                   .locus_lower = max_lower,
                   .locus_upper = min_upper,
               });
            }
         }
      }

      if (earliest_conflict.has_value())
      {
         return MergeResult{
             .ok       = false,
             .merged   = {},
             .conflict = std::move(earliest_conflict),
         };
      }

      // 2. Conflict-free 3-way state splice: apply tx_b's write intervals onto tx_a.post_root
      ArtRoot merged = tx_a.post_root;
      for (const auto& rb : cs_b.ranges())
      {
         if (!rb.is_write || rb.lower.empty())
            continue;

         const auto domain = std::to_integer<std::uint8_t>(rb.lower[0]);
         if (domain == 0x00u)
         {
            std::span<const std::byte> raw_lower{rb.lower.data() + 1, rb.lower.size() - 1};
            std::span<const std::byte> raw_upper{};
            if (!rb.upper.empty() && std::to_integer<std::uint8_t>(rb.upper[0]) == 0x00u)
            {
               raw_upper = std::span<const std::byte>{rb.upper.data() + 1, rb.upper.size() - 1};
            }
            merged = splice_raw_range(art, std::move(merged), tx_b.post_root, raw_lower, raw_upper,
                                      /*bytes_only=*/true);
         }
         else if (domain == 0x01u && rb.lower.size() >= 3)
         {
            const std::uint16_t slen =
                (static_cast<std::uint16_t>(std::to_integer<std::uint8_t>(rb.lower[1])) << 8) |
                static_cast<std::uint16_t>(std::to_integer<std::uint8_t>(rb.lower[2]));
            const std::size_t hdr_len = 3u + slen;
            if (rb.lower.size() < hdr_len)
               continue;

            std::span<const std::byte> sub_key{rb.lower.data() + 3, slen};
            std::span<const std::byte> ns_prefix{rb.lower.data(), hdr_len};
            std::span<const std::byte> inner_lower{rb.lower.data() + hdr_len,
                                                   rb.lower.size() - hdr_len};
            std::span<const std::byte> inner_upper{};
            if (!rb.upper.empty() && starts_with(rb.upper, ns_prefix))
            {
               inner_upper = std::span<const std::byte>{rb.upper.data() + hdr_len,
                                                        rb.upper.size() - hdr_len};
            }

            ArtRoot dst_sub = art.get_subtree(merged, sub_key).value_or(ArtRoot{});
            ArtRoot src_sub = art.get_subtree(tx_b.post_root, sub_key).value_or(ArtRoot{});
            dst_sub = splice_raw_range(art, std::move(dst_sub), src_sub, inner_lower, inner_upper,
                                       /*bytes_only=*/false);
            merged  = art.upsert_subtree(merged, sub_key, dst_sub);
         }
      }

      ChangeSet merged_changes = cs_a;
      merged_changes.append_all(cs_b);
      merged_changes.normalize();

      Checksum256 mroot = merkle ? merkle->compute_root(merged) : Checksum256{};
      return MergeResult{
          .ok          = true,
          .merged      = CommittedTx{.base_root = base_root,
                                     .post_root = std::move(merged),
                                     .changes   = std::move(merged_changes)},
          .merkle_root = mroot,
          .conflict    = std::nullopt,
      };
   }
}  // namespace muabase_db
