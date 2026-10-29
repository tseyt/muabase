#include <muabase_db/cow_art.hpp>

#include <algorithm>
#include <cassert>
#include <cstring>
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

      PtrAddress decode_subtree_addr(std::span<const std::byte> val)
      {
         if (val.size() != sizeof(std::uint32_t))
            throw std::runtime_error("Invalid subtree value size");
         std::uint32_t raw = 0;
         std::memcpy(&raw, val.data(), sizeof(raw));
         return PtrAddress{raw};
      }

      struct DecodedLeaf
      {
         std::vector<KvEntry> entries;
      };

      struct DecodedInner
      {
         std::uint64_t                                  subtree_count = 0;
         std::vector<std::byte>                         prefix;
         bool                                           has_terminal = false;
         ValueTag                                       terminal_tag = ValueTag::Bytes;
         std::vector<std::byte>                         terminal_val;
         std::vector<std::pair<std::uint8_t, PtrAddress>> children;  // sorted ascending by byte
      };

      std::size_t estimate_leaf_bytes(const std::vector<KvEntry>& entries) noexcept
      {
         std::size_t total = 4;  // num_entries (u16) + reserved (u16)
         for (const auto& e : entries)
         {
            total += 8 + e.key.size() + e.value.size();
         }
         return total;
      }

      std::vector<std::byte> encode_leaf(const std::vector<KvEntry>& entries)
      {
         std::vector<std::byte> buf(estimate_leaf_bytes(entries));
         std::byte*             p   = buf.data();
         const auto             cnt = static_cast<std::uint16_t>(entries.size());
         const std::uint16_t    pad = 0;
         std::memcpy(p, &cnt, 2);
         std::memcpy(p + 2, &pad, 2);
         p += 4;

         for (const auto& e : entries)
         {
            const auto     klen = static_cast<std::uint16_t>(e.key.size());
            const auto     tag  = static_cast<std::uint8_t>(e.tag);
            const uint8_t  zpad = 0;
            const auto     vlen = static_cast<std::uint32_t>(e.value.size());
            std::memcpy(p, &klen, 2);
            p[2] = static_cast<std::byte>(tag);
            p[3] = static_cast<std::byte>(zpad);
            std::memcpy(p + 4, &vlen, 4);
            p += 8;
            if (klen > 0)
            {
               std::memcpy(p, e.key.data(), klen);
               p += klen;
            }
            if (vlen > 0)
            {
               std::memcpy(p, e.value.data(), vlen);
               p += vlen;
            }
         }
         return buf;
      }

      DecodedLeaf decode_leaf(std::span<const std::byte> payload)
      {
         if (payload.size() < 4)
            throw std::runtime_error("Corrupt BTreeLeaf header");
         std::uint16_t cnt = 0;
         std::memcpy(&cnt, payload.data(), 2);

         DecodedLeaf out;
         out.entries.reserve(cnt);
         std::size_t off = 4;
         for (std::uint16_t i = 0; i < cnt; ++i)
         {
            if (off + 8 > payload.size())
               throw std::runtime_error("Corrupt BTreeLeaf entry header");
            std::uint16_t klen = 0;
            std::uint32_t vlen = 0;
            std::memcpy(&klen, payload.data() + off, 2);
            auto tag = static_cast<ValueTag>(std::to_integer<std::uint8_t>(payload[off + 2]));
            std::memcpy(&vlen, payload.data() + off + 4, 4);
            off += 8;
            if (off + klen + vlen > payload.size())
               throw std::runtime_error("Corrupt BTreeLeaf entry payload");

            KvEntry e;
            e.tag = tag;
            e.key.assign(payload.begin() + static_cast<std::ptrdiff_t>(off),
                         payload.begin() + static_cast<std::ptrdiff_t>(off + klen));
            off += klen;
            e.value.assign(payload.begin() + static_cast<std::ptrdiff_t>(off),
                           payload.begin() + static_cast<std::ptrdiff_t>(off + vlen));
            off += vlen;
            out.entries.push_back(std::move(e));
         }
         return out;
      }

      // Wire format for ArtInner (fixed 20-byte header):
      //   [0..8)   u64 subtree_count
      //   [8..10)  u16 prefix_len
      //   [10..12) u16 num_children (0..256)
      //   [12]     u8  has_terminal
      //   [13]     u8  terminal_tag
      //   [14..16) u16 reserved
      //   [16..20) u32 terminal_val_len
      //   prefix[prefix_len]
      //   terminal_val[terminal_val_len]
      //   branch_bytes[num_children]
      //   pad to 4B alignment
      //   child_addrs[num_children] (u32 each)
      std::vector<std::byte> encode_inner(const DecodedInner& in)
      {
         const auto        plen = static_cast<std::uint16_t>(in.prefix.size());
         const auto        nc   = static_cast<std::uint16_t>(in.children.size());
         const auto        tlen = static_cast<std::uint32_t>(in.has_terminal ? in.terminal_val.size() : 0u);
         const std::size_t unaligned = 20u + plen + tlen + nc;
         const std::size_t aligned   = (unaligned + 3u) & ~std::size_t{3u};
         const std::size_t total     = aligned + static_cast<std::size_t>(nc) * sizeof(std::uint32_t);

         std::vector<std::byte> buf(total, std::byte{0});
         std::byte*             p   = buf.data();
         const std::uint8_t     ht  = in.has_terminal ? 1u : 0u;
         const auto             tt  = static_cast<std::uint8_t>(in.terminal_tag);
         const std::uint16_t    res = 0;

         std::memcpy(p + 0, &in.subtree_count, 8);
         std::memcpy(p + 8, &plen, 2);
         std::memcpy(p + 10, &nc, 2);
         p[12] = static_cast<std::byte>(ht);
         p[13] = static_cast<std::byte>(tt);
         std::memcpy(p + 14, &res, 2);
         std::memcpy(p + 16, &tlen, 4);

         std::size_t off = 20;
         if (plen > 0)
         {
            std::memcpy(p + off, in.prefix.data(), plen);
            off += plen;
         }
         if (tlen > 0)
         {
            std::memcpy(p + off, in.terminal_val.data(), tlen);
            off += tlen;
         }
         for (std::uint16_t i = 0; i < nc; ++i)
         {
            p[off + i] = static_cast<std::byte>(in.children[i].first);
         }
         std::size_t addr_off = aligned;
         for (std::uint16_t i = 0; i < nc; ++i)
         {
            const std::uint32_t raw_addr = in.children[i].second.value;
            std::memcpy(p + addr_off + i * sizeof(std::uint32_t), &raw_addr, sizeof(std::uint32_t));
         }
         return buf;
      }

      DecodedInner decode_inner(std::span<const std::byte> payload)
      {
         if (payload.size() < 20)
            throw std::runtime_error("Corrupt ArtInner header");

         DecodedInner  out;
         std::uint16_t plen = 0;
         std::uint16_t nc   = 0;
         std::uint32_t tlen = 0;
         std::memcpy(&out.subtree_count, payload.data() + 0, 8);
         std::memcpy(&plen, payload.data() + 8, 2);
         std::memcpy(&nc, payload.data() + 10, 2);
         out.has_terminal = std::to_integer<std::uint8_t>(payload[12]) != 0;
         out.terminal_tag = static_cast<ValueTag>(std::to_integer<std::uint8_t>(payload[13]));
         std::memcpy(&tlen, payload.data() + 16, 4);

         const std::size_t unaligned = 20u + plen + tlen + nc;
         const std::size_t aligned   = (unaligned + 3u) & ~std::size_t{3u};
         const std::size_t total     = aligned + static_cast<std::size_t>(nc) * sizeof(std::uint32_t);
         if (payload.size() < total)
            throw std::runtime_error("Corrupt ArtInner body");

         std::size_t off = 20;
         out.prefix.assign(payload.begin() + static_cast<std::ptrdiff_t>(off),
                           payload.begin() + static_cast<std::ptrdiff_t>(off + plen));
         off += plen;

         if (tlen > 0)
         {
            out.terminal_val.assign(payload.begin() + static_cast<std::ptrdiff_t>(off),
                                    payload.begin() + static_cast<std::ptrdiff_t>(off + tlen));
            off += tlen;
         }

         out.children.reserve(nc);
         for (std::uint16_t i = 0; i < nc; ++i)
         {
            auto          b        = std::to_integer<std::uint8_t>(payload[off + i]);
            std::uint32_t raw_addr = 0;
            std::memcpy(&raw_addr, payload.data() + aligned + i * sizeof(std::uint32_t),
                        sizeof(std::uint32_t));
            out.children.emplace_back(b, PtrAddress{raw_addr});
         }
         return out;
      }

      PtrAddress alloc_leaf_retaining_subtrees(SegmentAllocator&           sal,
                                               const std::vector<KvEntry>& entries,
                                               PtrAddress                  hint = {})
      {
         for (const auto& e : entries)
         {
            if (e.tag == ValueTag::Subtree)
            {
               auto sub = decode_subtree_addr(e.value);
               if (sub)
                  sal.retain(sub, 1);
            }
         }
         auto encoded = encode_leaf(entries);
         return sal.allocate(encoded, ObjectKind::BTreeLeaf, hint);
      }

      // Assumes children in `in.children` ALREADY hold the exact ref owned by this new node,
      // and retains `terminal_val` iff `in.has_terminal && in.terminal_tag == ValueTag::Subtree`
      // when `retain_terminal_subtree` is true.
      PtrAddress alloc_inner_node(SegmentAllocator&   sal,
                                  const DecodedInner& in,
                                  bool                retain_terminal_subtree,
                                  PtrAddress          hint = {})
      {
         if (retain_terminal_subtree && in.has_terminal && in.terminal_tag == ValueTag::Subtree)
         {
            auto sub = decode_subtree_addr(in.terminal_val);
            if (sub)
               sal.retain(sub, 1);
         }
         auto encoded = encode_inner(in);
         return sal.allocate(encoded, ObjectKind::ArtInner, hint);
      }

      std::uint64_t read_node_count(SegmentAllocator& sal, PtrAddress addr)
      {
         if (!addr)
            return 0;
         return sal.read(addr,
                         [](ObjectKind kind, std::span<const std::byte> payload) -> std::uint64_t
                         {
                            if (kind == ObjectKind::BTreeLeaf)
                            {
                               std::uint16_t cnt = 0;
                               std::memcpy(&cnt, payload.data(), 2);
                               return cnt;
                            }
                            if (kind == ObjectKind::ArtInner)
                            {
                               std::uint64_t cnt = 0;
                               std::memcpy(&cnt, payload.data(), 8);
                               return cnt;
                            }
                            return 0;
                         });
      }

      PtrAddress upsert_rec(SegmentAllocator&          sal,
                            PtrAddress                 node_addr,
                            std::size_t                depth,
                            std::span<const std::byte> key,
                            std::span<const std::byte> val,
                            ValueTag                   tag,
                            bool&                      inserted_new,
                            PtrAddress                 hint = {})
      {
         if (!node_addr)
         {
            inserted_new = true;
            KvEntry e{
                .key   = {key.begin(), key.end()},
                .value = {val.begin(), val.end()},
                .tag   = tag,
            };
            return alloc_leaf_retaining_subtrees(sal, {std::move(e)}, hint);
         }

         ObjectKind             kind{};
         std::vector<std::byte> raw;
         sal.read(node_addr,
                  [&](ObjectKind k, std::span<const std::byte> p)
                  {
                     kind = k;
                     raw.assign(p.begin(), p.end());
                  });

         if (kind == ObjectKind::BTreeLeaf)
         {
            DecodedLeaf leaf = decode_leaf(raw);
            auto        it   = std::lower_bound(leaf.entries.begin(), leaf.entries.end(), key,
                                                [](const KvEntry& e, std::span<const std::byte> k)
                                                { return lex_compare(e.key, k) < 0; });

            if (it != leaf.entries.end() && lex_compare(it->key, key) == 0)
            {
               inserted_new = false;
               it->value.assign(val.begin(), val.end());
               it->tag = tag;
               if (leaf.entries.size() <= CowArt::kMaxLeafEntries &&
                   estimate_leaf_bytes(leaf.entries) <= CowArt::kMaxLeafBytes)
               {
                  return alloc_leaf_retaining_subtrees(sal, leaf.entries, hint);
               }
            }
            else
            {
               inserted_new = true;
               leaf.entries.insert(it, KvEntry{
                                           .key   = {key.begin(), key.end()},
                                           .value = {val.begin(), val.end()},
                                           .tag   = tag,
                                       });
               if (leaf.entries.size() <= CowArt::kMaxLeafEntries &&
                   estimate_leaf_bytes(leaf.entries) <= CowArt::kMaxLeafBytes)
               {
                  return alloc_leaf_retaining_subtrees(sal, leaf.entries, hint);
               }
            }

            // Split overflowing BTreeLeaf into an ArtInner at `depth` with path compression
            const auto& first_k = leaf.entries.front().key;
            const auto& last_k  = leaf.entries.back().key;
            std::size_t max_lcp = std::min(first_k.size(), last_k.size());
            std::size_t lcp     = 0;
            while (depth + lcp < max_lcp && first_k[depth + lcp] == last_k[depth + lcp])
            {
               ++lcp;
            }

            DecodedInner inner;
            inner.subtree_count = leaf.entries.size();
            inner.prefix.assign(first_k.begin() + static_cast<std::ptrdiff_t>(depth),
                                first_k.begin() + static_cast<std::ptrdiff_t>(depth + lcp));

            std::size_t idx       = 0;
            const auto  split_pos = depth + lcp;
            if (leaf.entries[0].key.size() == split_pos)
            {
               inner.has_terminal = true;
               inner.terminal_tag = leaf.entries[0].tag;
               inner.terminal_val = leaf.entries[0].value;
               idx                = 1;
            }

            PtrAddress sibling_hint = hint;
            while (idx < leaf.entries.size())
            {
               const auto           branch_b = std::to_integer<std::uint8_t>(leaf.entries[idx].key[split_pos]);
               std::vector<KvEntry> bucket;
               while (idx < leaf.entries.size() &&
                      std::to_integer<std::uint8_t>(leaf.entries[idx].key[split_pos]) == branch_b)
               {
                  bucket.push_back(std::move(leaf.entries[idx]));
                  ++idx;
               }
               PtrAddress child_addr = alloc_leaf_retaining_subtrees(sal, bucket, sibling_hint);
               if (!sibling_hint)
                  sibling_hint = child_addr;
               inner.children.emplace_back(branch_b, child_addr);
            }
            return alloc_inner_node(sal, inner, /*retain_terminal_subtree=*/true, hint);
         }

         // ArtInner
         DecodedInner in   = decode_inner(raw);
         std::size_t  plen = in.prefix.size();
         std::size_t  m    = 0;
         while (m < plen && (depth + m) < key.size() && in.prefix[m] == key[depth + m])
         {
            ++m;
         }

         if (m < plen)
         {
            // Prefix mismatch at offset m: split `in` into `parent -> split_child` (+ optional new leaf)
            inserted_new = true;

            DecodedInner split_child;
            split_child.subtree_count = in.subtree_count;
            split_child.prefix.assign(in.prefix.begin() + static_cast<std::ptrdiff_t>(m + 1),
                                      in.prefix.end());
            split_child.has_terminal = in.has_terminal;
            split_child.terminal_tag = in.terminal_tag;
            split_child.terminal_val = std::move(in.terminal_val);
            split_child.children     = in.children;
            for (const auto& [_, caddr] : split_child.children)
            {
               sal.retain(caddr, 1);
            }
            const auto old_b            = std::to_integer<std::uint8_t>(in.prefix[m]);
            PtrAddress split_child_addr =
                alloc_inner_node(sal, split_child, /*retain_terminal_subtree=*/true, hint);

            DecodedInner parent;
            parent.subtree_count = in.subtree_count + 1;
            parent.prefix.assign(in.prefix.begin(),
                                 in.prefix.begin() + static_cast<std::ptrdiff_t>(m));

            if (depth + m == key.size())
            {
               parent.has_terminal = true;
               parent.terminal_tag = tag;
               parent.terminal_val.assign(val.begin(), val.end());
               parent.children.emplace_back(old_b, split_child_addr);
            }
            else
            {
               const auto new_b = std::to_integer<std::uint8_t>(key[depth + m]);
               KvEntry    new_entry{
                      .key   = {key.begin(), key.end()},
                      .value = {val.begin(), val.end()},
                      .tag   = tag,
               };
               PtrAddress new_leaf =
                   alloc_leaf_retaining_subtrees(sal, {std::move(new_entry)}, split_child_addr);
               if (old_b < new_b)
               {
                  parent.children.emplace_back(old_b, split_child_addr);
                  parent.children.emplace_back(new_b, new_leaf);
               }
               else
               {
                  parent.children.emplace_back(new_b, new_leaf);
                  parent.children.emplace_back(old_b, split_child_addr);
               }
            }
            return alloc_inner_node(sal, parent, /*retain_terminal_subtree=*/true, hint);
         }

         // Full prefix matched
         const std::size_t next_depth = depth + plen;
         if (next_depth == key.size())
         {
            inserted_new = !in.has_terminal;
            for (const auto& [_, caddr] : in.children)
            {
               sal.retain(caddr, 1);
            }
            if (inserted_new)
               in.subtree_count += 1;
            in.has_terminal = true;
            in.terminal_tag = tag;
            in.terminal_val.assign(val.begin(), val.end());
            return alloc_inner_node(sal, in, /*retain_terminal_subtree=*/true, hint);
         }

         const auto branch_b = std::to_integer<std::uint8_t>(key[next_depth]);
         auto       cit = std::lower_bound(in.children.begin(), in.children.end(), branch_b,
                                           [](const auto& pair, std::uint8_t b) { return pair.first < b; });
         PtrAddress sib_hint = in.children.empty() ? hint : in.children.front().second;

         if (cit != in.children.end() && cit->first == branch_b)
         {
            const std::size_t target_idx = static_cast<std::size_t>(cit - in.children.begin());
            PtrAddress new_child = upsert_rec(sal, cit->second, next_depth + 1, key, val, tag,
                                              inserted_new, sib_hint);
            for (std::size_t i = 0; i < in.children.size(); ++i)
            {
               if (i != target_idx)
                  sal.retain(in.children[i].second, 1);
            }
            in.children[target_idx].second = new_child;
            if (inserted_new)
               in.subtree_count += 1;
            return alloc_inner_node(sal, in, /*retain_terminal_subtree=*/true, hint);
         }

         inserted_new = true;
         for (const auto& [_, caddr] : in.children)
         {
            sal.retain(caddr, 1);
         }
         KvEntry new_entry{
             .key   = {key.begin(), key.end()},
             .value = {val.begin(), val.end()},
             .tag   = tag,
         };
         PtrAddress new_child =
             alloc_leaf_retaining_subtrees(sal, {std::move(new_entry)}, sib_hint);
         in.children.insert(cit, {branch_b, new_child});
         in.subtree_count += 1;
         return alloc_inner_node(sal, in, /*retain_terminal_subtree=*/true, hint);
      }

      std::pair<PtrAddress, bool> remove_rec(SegmentAllocator&          sal,
                                             PtrAddress                 node_addr,
                                             std::size_t                depth,
                                             std::span<const std::byte> key)
      {
         if (!node_addr)
            return {{}, false};

         ObjectKind             kind{};
         std::vector<std::byte> raw;
         sal.read(node_addr,
                  [&](ObjectKind k, std::span<const std::byte> p)
                  {
                     kind = k;
                     raw.assign(p.begin(), p.end());
                  });

         if (kind == ObjectKind::BTreeLeaf)
         {
            DecodedLeaf leaf = decode_leaf(raw);
            auto        it   = std::lower_bound(leaf.entries.begin(), leaf.entries.end(), key,
                                                [](const KvEntry& e, std::span<const std::byte> k)
                                                { return lex_compare(e.key, k) < 0; });
            if (it == leaf.entries.end() || lex_compare(it->key, key) != 0)
               return {{}, false};

            if (leaf.entries.size() == 1)
               return {{}, true};

            leaf.entries.erase(it);
            return {alloc_leaf_retaining_subtrees(sal, leaf.entries), true};
         }

         DecodedInner in   = decode_inner(raw);
         std::size_t  plen = in.prefix.size();
         if (key.size() < depth + plen)
            return {{}, false};
         if (plen > 0 && std::memcmp(in.prefix.data(), key.data() + depth, plen) != 0)
            return {{}, false};

         const std::size_t next_depth = depth + plen;
         if (next_depth == key.size())
         {
            if (!in.has_terminal)
               return {{}, false};
            if (in.children.empty())
               return {{}, true};

            for (const auto& [_, caddr] : in.children)
            {
               sal.retain(caddr, 1);
            }
            in.has_terminal = false;
            in.terminal_val.clear();
            in.subtree_count -= 1;
            return {alloc_inner_node(sal, in, /*retain_terminal_subtree=*/false), true};
         }

         const auto branch_b = std::to_integer<std::uint8_t>(key[next_depth]);
         auto       cit = std::lower_bound(in.children.begin(), in.children.end(), branch_b,
                                           [](const auto& pair, std::uint8_t b) { return pair.first < b; });
         if (cit == in.children.end() || cit->first != branch_b)
            return {{}, false};

         const std::size_t target_idx = static_cast<std::size_t>(cit - in.children.begin());
         auto [new_child, removed]    = remove_rec(sal, cit->second, next_depth + 1, key);
         if (!removed)
            return {{}, false};

         if (!new_child && !in.has_terminal && in.children.size() == 1)
            return {{}, true};

         for (std::size_t i = 0; i < in.children.size(); ++i)
         {
            if (i != target_idx)
               sal.retain(in.children[i].second, 1);
         }
         if (new_child)
         {
            in.children[target_idx].second = new_child;
         }
         else
         {
            in.children.erase(in.children.begin() + static_cast<std::ptrdiff_t>(target_idx));
         }
         in.subtree_count -= 1;
         return {alloc_inner_node(sal, in, /*retain_terminal_subtree=*/true), true};
      }

      std::optional<KvEntry> min_entry(SegmentAllocator&       sal,
                                       PtrAddress              node_addr,
                                       std::vector<std::byte>& path_buf)
      {
         if (!node_addr)
            return std::nullopt;

         ObjectKind             kind{};
         std::vector<std::byte> raw;
         sal.read(node_addr,
                  [&](ObjectKind k, std::span<const std::byte> p)
                  {
                     kind = k;
                     raw.assign(p.begin(), p.end());
                  });

         if (kind == ObjectKind::BTreeLeaf)
         {
            DecodedLeaf leaf = decode_leaf(raw);
            if (leaf.entries.empty())
               return std::nullopt;
            return std::move(leaf.entries.front());
         }

         DecodedInner      in         = decode_inner(raw);
         const std::size_t saved_size = path_buf.size();
         path_buf.insert(path_buf.end(), in.prefix.begin(), in.prefix.end());

         if (in.has_terminal)
         {
            KvEntry out{.key = path_buf, .value = std::move(in.terminal_val), .tag = in.terminal_tag};
            path_buf.resize(saved_size);
            return out;
         }
         for (const auto& [b, caddr] : in.children)
         {
            path_buf.push_back(static_cast<std::byte>(b));
            auto res = min_entry(sal, caddr, path_buf);
            path_buf.pop_back();
            if (res.has_value())
            {
               path_buf.resize(saved_size);
               return res;
            }
         }
         path_buf.resize(saved_size);
         return std::nullopt;
      }

      std::optional<KvEntry> max_entry(SegmentAllocator&       sal,
                                       PtrAddress              node_addr,
                                       std::vector<std::byte>& path_buf)
      {
         if (!node_addr)
            return std::nullopt;

         ObjectKind             kind{};
         std::vector<std::byte> raw;
         sal.read(node_addr,
                  [&](ObjectKind k, std::span<const std::byte> p)
                  {
                     kind = k;
                     raw.assign(p.begin(), p.end());
                  });

         if (kind == ObjectKind::BTreeLeaf)
         {
            DecodedLeaf leaf = decode_leaf(raw);
            if (leaf.entries.empty())
               return std::nullopt;
            return std::move(leaf.entries.back());
         }

         DecodedInner      in         = decode_inner(raw);
         const std::size_t saved_size = path_buf.size();
         path_buf.insert(path_buf.end(), in.prefix.begin(), in.prefix.end());

         for (auto it = in.children.rbegin(); it != in.children.rend(); ++it)
         {
            path_buf.push_back(static_cast<std::byte>(it->first));
            auto res = max_entry(sal, it->second, path_buf);
            path_buf.pop_back();
            if (res.has_value())
            {
               path_buf.resize(saved_size);
               return res;
            }
         }
         if (in.has_terminal)
         {
            KvEntry out{.key = path_buf, .value = std::move(in.terminal_val), .tag = in.terminal_tag};
            path_buf.resize(saved_size);
            return out;
         }
         path_buf.resize(saved_size);
         return std::nullopt;
      }

      std::optional<KvEntry> find_ge(SegmentAllocator&          sal,
                                     PtrAddress                 node_addr,
                                     std::size_t                depth,
                                     std::span<const std::byte> key,
                                     std::vector<std::byte>&    path_buf)
      {
         if (!node_addr)
            return std::nullopt;

         if (depth >= key.size())
            return min_entry(sal, node_addr, path_buf);

         ObjectKind             kind{};
         std::vector<std::byte> raw;
         sal.read(node_addr,
                  [&](ObjectKind k, std::span<const std::byte> p)
                  {
                     kind = k;
                     raw.assign(p.begin(), p.end());
                  });

         if (kind == ObjectKind::BTreeLeaf)
         {
            DecodedLeaf leaf = decode_leaf(raw);
            auto        it   = std::lower_bound(leaf.entries.begin(), leaf.entries.end(), key,
                                                [](const KvEntry& e, std::span<const std::byte> k)
                                                { return lex_compare(e.key, k) < 0; });
            if (it == leaf.entries.end())
               return std::nullopt;
            return std::move(*it);
         }

         DecodedInner       in         = decode_inner(raw);
         const auto         key_rem    = key.subspan(depth);
         const std::size_t  plen       = in.prefix.size();
         const std::size_t  cmp_len    = std::min(plen, key_rem.size());
         const int          c          = cmp_len > 0 ? std::memcmp(in.prefix.data(), key_rem.data(), cmp_len) : 0;
         const std::size_t  saved_size = path_buf.size();
         path_buf.insert(path_buf.end(), in.prefix.begin(), in.prefix.end());

         auto restore_and = [&](std::optional<KvEntry> r)
         {
            path_buf.resize(saved_size);
            return r;
         };

         if (c > 0)
         {
            path_buf.resize(saved_size);
            return min_entry(sal, node_addr, path_buf);
         }
         if (c < 0)
         {
            return restore_and(std::nullopt);
         }
         if (plen >= key_rem.size())
         {
            path_buf.resize(saved_size);
            return min_entry(sal, node_addr, path_buf);
         }

         const auto branch_b = std::to_integer<std::uint8_t>(key[depth + plen]);
         auto       cit = std::lower_bound(in.children.begin(), in.children.end(), branch_b,
                                           [](const auto& pair, std::uint8_t b) { return pair.first < b; });
         if (cit == in.children.end())
            return restore_and(std::nullopt);

         if (cit->first == branch_b)
         {
            path_buf.push_back(static_cast<std::byte>(cit->first));
            auto exact = find_ge(sal, cit->second, depth + plen + 1, key, path_buf);
            path_buf.pop_back();
            if (exact.has_value())
               return restore_and(std::move(exact));
            ++cit;
         }

         if (cit != in.children.end())
         {
            path_buf.push_back(static_cast<std::byte>(cit->first));
            auto next = min_entry(sal, cit->second, path_buf);
            path_buf.pop_back();
            return restore_and(std::move(next));
         }
         return restore_and(std::nullopt);
      }

      std::optional<KvEntry> find_lt(SegmentAllocator&          sal,
                                     PtrAddress                 node_addr,
                                     std::size_t                depth,
                                     std::span<const std::byte> key,
                                     std::vector<std::byte>&    path_buf)
      {
         if (!node_addr || depth >= key.size())
            return std::nullopt;

         ObjectKind             kind{};
         std::vector<std::byte> raw;
         sal.read(node_addr,
                  [&](ObjectKind k, std::span<const std::byte> p)
                  {
                     kind = k;
                     raw.assign(p.begin(), p.end());
                  });

         if (kind == ObjectKind::BTreeLeaf)
         {
            DecodedLeaf leaf = decode_leaf(raw);
            auto        it   = std::lower_bound(leaf.entries.begin(), leaf.entries.end(), key,
                                                [](const KvEntry& e, std::span<const std::byte> k)
                                                { return lex_compare(e.key, k) < 0; });
            if (it == leaf.entries.begin())
               return std::nullopt;
            --it;
            return std::move(*it);
         }

         DecodedInner      in         = decode_inner(raw);
         const auto        key_rem    = key.subspan(depth);
         const std::size_t plen       = in.prefix.size();
         const std::size_t cmp_len    = std::min(plen, key_rem.size());
         const int         c          = cmp_len > 0 ? std::memcmp(in.prefix.data(), key_rem.data(), cmp_len) : 0;
         const std::size_t saved_size = path_buf.size();
         path_buf.insert(path_buf.end(), in.prefix.begin(), in.prefix.end());

         auto restore_and = [&](std::optional<KvEntry> r)
         {
            path_buf.resize(saved_size);
            return r;
         };

         if (c < 0)
         {
            path_buf.resize(saved_size);
            return max_entry(sal, node_addr, path_buf);
         }
         if (c > 0 || plen >= key_rem.size())
         {
            return restore_and(std::nullopt);
         }

         const auto branch_b = std::to_integer<std::uint8_t>(key[depth + plen]);
         auto       cit = std::lower_bound(in.children.begin(), in.children.end(), branch_b,
                                           [](const auto& pair, std::uint8_t b) { return pair.first < b; });
         if (cit != in.children.end() && cit->first == branch_b)
         {
            path_buf.push_back(static_cast<std::byte>(cit->first));
            auto sub = find_lt(sal, cit->second, depth + plen + 1, key, path_buf);
            path_buf.pop_back();
            if (sub.has_value())
               return restore_and(std::move(sub));
         }

         if (cit != in.children.begin())
         {
            --cit;
            path_buf.push_back(static_cast<std::byte>(cit->first));
            auto prev = max_entry(sal, cit->second, path_buf);
            path_buf.pop_back();
            if (prev.has_value())
               return restore_and(std::move(prev));
         }

         if (in.has_terminal)
         {
            KvEntry term{.key = path_buf, .value = std::move(in.terminal_val), .tag = in.terminal_tag};
            return restore_and(std::move(term));
         }
         return restore_and(std::nullopt);
      }

      // Prefix interval predicates for any subtree whose keys all start with `pfx`:
      //   1. prefix_all_lt_lower: every possible extension of `pfx` is strictly < `lower`
      //   2. prefix_all_ge_lower: every possible extension of `pfx` is >= `lower`
      //   3. prefix_all_ge_upper: every possible extension of `pfx` is >= `upper`
      //   4. prefix_all_lt_upper: every possible extension of `pfx` is strictly < `upper`
      bool prefix_all_lt(std::span<const std::byte> pfx, std::span<const std::byte> bound) noexcept
      {
         const std::size_t n = std::min(pfx.size(), bound.size());
         if (n == 0)
            return false;
         return std::memcmp(pfx.data(), bound.data(), n) < 0;
      }

      bool prefix_all_ge(std::span<const std::byte> pfx, std::span<const std::byte> bound) noexcept
      {
         return lex_compare(pfx, bound) >= 0;
      }

      bool key_in_range(std::span<const std::byte> key,
                        std::span<const std::byte> lower,
                        std::span<const std::byte> upper) noexcept
      {
         if (lex_compare(key, lower) < 0)
            return false;
         if (!upper.empty() && lex_compare(key, upper) >= 0)
            return false;
         return true;
      }

      std::uint64_t count_range_rec(SegmentAllocator&          sal,
                                    PtrAddress                 node_addr,
                                    std::vector<std::byte>&    path_buf,
                                    std::span<const std::byte> lower,
                                    std::span<const std::byte> upper)
      {
         if (!node_addr)
            return 0;

         ObjectKind             kind{};
         std::vector<std::byte> raw;
         sal.read(node_addr,
                  [&](ObjectKind k, std::span<const std::byte> p)
                  {
                     kind = k;
                     raw.assign(p.begin(), p.end());
                  });

         if (kind == ObjectKind::BTreeLeaf)
         {
            DecodedLeaf   leaf = decode_leaf(raw);
            std::uint64_t cnt  = 0;
            for (const auto& e : leaf.entries)
            {
               if (key_in_range(e.key, lower, upper))
                  ++cnt;
            }
            return cnt;
         }

         DecodedInner      in         = decode_inner(raw);
         const std::size_t saved_size = path_buf.size();
         path_buf.insert(path_buf.end(), in.prefix.begin(), in.prefix.end());

         std::span<const std::byte> full_pfx{path_buf.data(), path_buf.size()};
         if (prefix_all_lt(full_pfx, lower) || (!upper.empty() && prefix_all_ge(full_pfx, upper)))
         {
            path_buf.resize(saved_size);
            return 0;
         }
         if (prefix_all_ge(full_pfx, lower) && (upper.empty() || prefix_all_lt(full_pfx, upper)))
         {
            path_buf.resize(saved_size);
            return in.subtree_count;
         }

         std::uint64_t total = 0;
         if (in.has_terminal && key_in_range(full_pfx, lower, upper))
            ++total;

         for (const auto& [b, caddr] : in.children)
         {
            path_buf.push_back(static_cast<std::byte>(b));
            std::span<const std::byte> child_pfx{path_buf.data(), path_buf.size()};
            if (!prefix_all_lt(child_pfx, lower) &&
                (upper.empty() || !prefix_all_ge(child_pfx, upper)))
            {
               total += count_range_rec(sal, caddr, path_buf, lower, upper);
            }
            path_buf.pop_back();
         }

         path_buf.resize(saved_size);
         return total;
      }

      // Returns {new_addr, removed_count}. When removed_count == 0, returns {node_addr, 0} WITHOUT
      // allocating a clone (caller retains node_addr if embedding in a cloned parent).
      std::pair<PtrAddress, std::uint64_t> remove_range_rec(SegmentAllocator&          sal,
                                                            PtrAddress                 node_addr,
                                                            std::vector<std::byte>&    path_buf,
                                                            std::span<const std::byte> lower,
                                                            std::span<const std::byte> upper)
      {
         if (!node_addr)
            return {{}, 0};

         ObjectKind             kind{};
         std::vector<std::byte> raw;
         sal.read(node_addr,
                  [&](ObjectKind k, std::span<const std::byte> p)
                  {
                     kind = k;
                     raw.assign(p.begin(), p.end());
                  });

         if (kind == ObjectKind::BTreeLeaf)
         {
            DecodedLeaf          leaf = decode_leaf(raw);
            std::vector<KvEntry> kept;
            kept.reserve(leaf.entries.size());
            std::uint64_t removed = 0;
            for (auto& e : leaf.entries)
            {
               if (key_in_range(e.key, lower, upper))
                  ++removed;
               else
                  kept.push_back(std::move(e));
            }
            if (removed == 0)
               return {node_addr, 0};
            if (kept.empty())
               return {{}, removed};
            return {alloc_leaf_retaining_subtrees(sal, kept), removed};
         }

         DecodedInner      in         = decode_inner(raw);
         const std::size_t saved_size = path_buf.size();
         path_buf.insert(path_buf.end(), in.prefix.begin(), in.prefix.end());

         std::span<const std::byte> full_pfx{path_buf.data(), path_buf.size()};
         if (prefix_all_lt(full_pfx, lower) || (!upper.empty() && prefix_all_ge(full_pfx, upper)))
         {
            path_buf.resize(saved_size);
            return {node_addr, 0};
         }
         if (prefix_all_ge(full_pfx, lower) && (upper.empty() || prefix_all_lt(full_pfx, upper)))
         {
            path_buf.resize(saved_size);
            return {{}, in.subtree_count};
         }

         std::uint64_t removed = 0;
         bool          keep_terminal = in.has_terminal;
         if (in.has_terminal && key_in_range(full_pfx, lower, upper))
         {
            keep_terminal = false;
            ++removed;
         }

         std::vector<std::pair<std::uint8_t, PtrAddress>> new_children;
         new_children.reserve(in.children.size());
         std::vector<bool> child_is_fresh;
         child_is_fresh.reserve(in.children.size());

         for (const auto& [b, caddr] : in.children)
         {
            path_buf.push_back(static_cast<std::byte>(b));
            std::span<const std::byte> child_pfx{path_buf.data(), path_buf.size()};
            if (prefix_all_lt(child_pfx, lower) ||
                (!upper.empty() && prefix_all_ge(child_pfx, upper)))
            {
               new_children.emplace_back(b, caddr);
               child_is_fresh.push_back(false);
            }
            else if (prefix_all_ge(child_pfx, lower) &&
                     (upper.empty() || prefix_all_lt(child_pfx, upper)))
            {
               removed += read_node_count(sal, caddr);
            }
            else
            {
               auto [nc, sub_rem] = remove_range_rec(sal, caddr, path_buf, lower, upper);
               removed += sub_rem;
               if (nc)
               {
                  new_children.emplace_back(b, nc);
                  child_is_fresh.push_back(sub_rem > 0);
               }
            }
            path_buf.pop_back();
         }

         path_buf.resize(saved_size);
         if (removed == 0)
            return {node_addr, 0};
         if (!keep_terminal && new_children.empty())
            return {{}, removed};

         for (std::size_t i = 0; i < new_children.size(); ++i)
         {
            if (!child_is_fresh[i])
               sal.retain(new_children[i].second, 1);
         }

         DecodedInner out;
         out.subtree_count = in.subtree_count - removed;
         out.prefix        = std::move(in.prefix);
         out.has_terminal  = keep_terminal;
         if (keep_terminal)
         {
            out.terminal_tag = in.terminal_tag;
            out.terminal_val = std::move(in.terminal_val);
         }
         out.children = std::move(new_children);
         return {alloc_inner_node(sal, out, /*retain_terminal_subtree=*/keep_terminal), removed};
      }
   }  // namespace

   ArtRoot::ArtRoot(SegmentAllocator* sal, PtrAddress addr, bool adopt_existing_ref) noexcept
       : _sal(sal), _addr(addr)
   {
      if (_sal && _addr && !adopt_existing_ref)
         _sal->retain(_addr, 1);
   }

   ArtRoot::ArtRoot(const ArtRoot& other) : _sal(other._sal), _addr(other._addr)
   {
      if (_sal && _addr)
         _sal->retain(_addr, 1);
   }

   ArtRoot::ArtRoot(ArtRoot&& other) noexcept : _sal(other._sal), _addr(other._addr)
   {
      other._addr = {};
   }

   ArtRoot& ArtRoot::operator=(const ArtRoot& other)
   {
      if (this != &other)
      {
         if (other._sal && other._addr)
            other._sal->retain(other._addr, 1);
         reset();
         _sal  = other._sal;
         _addr = other._addr;
      }
      return *this;
   }

   ArtRoot& ArtRoot::operator=(ArtRoot&& other) noexcept
   {
      if (this != &other)
      {
         reset();
         _sal        = other._sal;
         _addr       = other._addr;
         other._addr = {};
      }
      return *this;
   }

   ArtRoot::~ArtRoot()
   {
      reset();
   }

   void ArtRoot::reset() noexcept
   {
      if (_sal && _addr)
      {
         _sal->release(_addr);
         _addr = {};
      }
   }

   CowArt::CowArt(SegmentAllocator& sal) noexcept : _sal(sal) {}

   void CowArt::enumerate_node_children(ObjectKind                             kind,
                                        std::span<const std::byte>             payload,
                                        const std::function<void(PtrAddress)>& emit)
   {
      if (kind == ObjectKind::BTreeLeaf)
      {
         DecodedLeaf leaf = decode_leaf(payload);
         for (const auto& e : leaf.entries)
         {
            if (e.tag == ValueTag::Subtree)
            {
               auto sub = decode_subtree_addr(e.value);
               if (sub)
                  emit(sub);
            }
         }
      }
      else if (kind == ObjectKind::ArtInner)
      {
         DecodedInner in = decode_inner(payload);
         if (in.has_terminal && in.terminal_tag == ValueTag::Subtree)
         {
            auto sub = decode_subtree_addr(in.terminal_val);
            if (sub)
               emit(sub);
         }
         for (const auto& [_, caddr] : in.children)
         {
            if (caddr)
               emit(caddr);
         }
      }
   }

   std::size_t CowArt::drain_garbage(std::size_t max_steps)
   {
      return _sal.drain_deferred_releases(max_steps, &CowArt::enumerate_node_children);
   }

   void CowArt::visit_value(
       const ArtRoot&                                                   root,
       std::span<const std::byte>                                       key,
       const std::function<void(ValueTag, std::span<const std::byte>)>& cb) const
   {
      PtrAddress  cur   = root.address();
      std::size_t depth = 0;

      while (cur)
      {
         PtrAddress next_child{};
         bool       done = false;

         _sal.read(cur,
                   [&](ObjectKind kind, std::span<const std::byte> payload)
                   {
                      if (kind == ObjectKind::BTreeLeaf)
                      {
                         DecodedLeaf leaf = decode_leaf(payload);
                         auto        it   = std::lower_bound(
                             leaf.entries.begin(), leaf.entries.end(), key,
                             [](const KvEntry& e, std::span<const std::byte> k)
                             { return lex_compare(e.key, k) < 0; });
                         if (it != leaf.entries.end() && lex_compare(it->key, key) == 0)
                         {
                            cb(it->tag, it->value);
                         }
                         done = true;
                         return;
                      }

                      DecodedInner      in   = decode_inner(payload);
                      const std::size_t plen = in.prefix.size();
                      if (key.size() < depth + plen ||
                          (plen > 0 && std::memcmp(in.prefix.data(), key.data() + depth, plen) != 0))
                      {
                         done = true;
                         return;
                      }

                      depth += plen;
                      if (depth == key.size())
                      {
                         if (in.has_terminal)
                            cb(in.terminal_tag, in.terminal_val);
                         done = true;
                         return;
                      }

                      const auto branch_b = std::to_integer<std::uint8_t>(key[depth]);
                      auto cit = std::lower_bound(in.children.begin(), in.children.end(), branch_b,
                                                  [](const auto& pair, std::uint8_t b)
                                                  { return pair.first < b; });
                      if (cit == in.children.end() || cit->first != branch_b)
                      {
                         done = true;
                         return;
                      }
                      next_child = cit->second;
                      depth += 1;
                   });

         if (done)
            return;
         cur = next_child;
      }
   }

   std::optional<std::vector<std::byte>> CowArt::get(const ArtRoot&             root,
                                                     std::span<const std::byte> key,
                                                     ValueTag*                  out_tag) const
   {
      std::optional<std::vector<std::byte>> out;
      visit_value(root, key,
                  [&](ValueTag tag, std::span<const std::byte> val)
                  {
                     if (out_tag)
                        *out_tag = tag;
                     out.emplace(val.begin(), val.end());
                  });
      return out;
   }

   ArtRoot CowArt::upsert(const ArtRoot&             root,
                          std::span<const std::byte> key,
                          std::span<const std::byte> value,
                          ValueTag                   tag)
   {
      bool       inserted = false;
      PtrAddress next     = upsert_rec(_sal, root.address(), 0, key, value, tag, inserted);
      return ArtRoot{&_sal, next, /*adopt_existing_ref=*/true};
   }

   ArtRoot CowArt::upsert_subtree(const ArtRoot&             root,
                                  std::span<const std::byte> key,
                                  const ArtRoot&             child_root)
   {
      if (!child_root)
      {
         return remove(root, key).first;
      }
      const std::uint32_t raw_addr = child_root.address().value;
      std::byte           buf[sizeof(raw_addr)];
      std::memcpy(buf, &raw_addr, sizeof(raw_addr));
      return upsert(root, key, buf, ValueTag::Subtree);
   }

   std::optional<ArtRoot> CowArt::get_subtree(const ArtRoot&             root,
                                              std::span<const std::byte> key) const
   {
      ValueTag tag   = ValueTag::Bytes;
      auto     bytes = get(root, key, &tag);
      if (!bytes || tag != ValueTag::Subtree)
         return std::nullopt;
      PtrAddress addr = decode_subtree_addr(*bytes);
      if (!addr)
         return std::nullopt;
      return ArtRoot{&_sal, addr, /*adopt_existing_ref=*/false};
   }

   std::pair<ArtRoot, bool> CowArt::remove(const ArtRoot& root, std::span<const std::byte> key)
   {
      auto [next_addr, removed] = remove_rec(_sal, root.address(), 0, key);
      if (!removed)
         return {root, false};
      return {ArtRoot{&_sal, next_addr, /*adopt_existing_ref=*/true}, true};
   }

   std::optional<KvEntry> CowArt::lower_bound(const ArtRoot&             root,
                                              std::span<const std::byte> key,
                                              std::size_t                prefix_len) const
   {
      std::vector<std::byte> path_buf;
      auto                   res = find_ge(_sal, root.address(), 0, key, path_buf);
      if (!res.has_value())
         return std::nullopt;
      if (prefix_len > 0)
      {
         const std::size_t clamped = std::min(prefix_len, key.size());
         if (!starts_with(res->key, key.subspan(0, clamped)))
            return std::nullopt;
      }
      return res;
   }

   std::optional<KvEntry> CowArt::less_than(const ArtRoot&             root,
                                            std::span<const std::byte> key,
                                            std::size_t                prefix_len) const
   {
      std::vector<std::byte> path_buf;
      auto                   res = find_lt(_sal, root.address(), 0, key, path_buf);
      if (!res.has_value())
         return std::nullopt;
      if (prefix_len > 0)
      {
         const std::size_t clamped = std::min(prefix_len, key.size());
         if (!starts_with(res->key, key.subspan(0, clamped)))
            return std::nullopt;
      }
      return res;
   }

   std::optional<KvEntry> CowArt::max_with_prefix(const ArtRoot&             root,
                                                  std::span<const std::byte> prefix) const
   {
      PtrAddress             cur   = root.address();
      std::size_t            depth = 0;
      std::vector<std::byte> path_buf;

      while (cur)
      {
         if (depth >= prefix.size())
            return max_entry(_sal, cur, path_buf);

         ObjectKind             kind{};
         std::vector<std::byte> raw;
         _sal.read(cur,
                   [&](ObjectKind k, std::span<const std::byte> p)
                   {
                      kind = k;
                      raw.assign(p.begin(), p.end());
                   });

         if (kind == ObjectKind::BTreeLeaf)
         {
            DecodedLeaf leaf = decode_leaf(raw);
            for (auto it = leaf.entries.rbegin(); it != leaf.entries.rend(); ++it)
            {
               if (starts_with(it->key, prefix))
                  return std::move(*it);
            }
            return std::nullopt;
         }

         DecodedInner      in      = decode_inner(raw);
         const auto        pfx_rem = prefix.subspan(depth);
         const std::size_t plen    = in.prefix.size();
         const std::size_t cmp_len = std::min(plen, pfx_rem.size());
         if (cmp_len > 0 && std::memcmp(in.prefix.data(), pfx_rem.data(), cmp_len) != 0)
            return std::nullopt;

         path_buf.insert(path_buf.end(), in.prefix.begin(), in.prefix.end());
         if (plen >= pfx_rem.size())
         {
            path_buf.resize(path_buf.size() - plen);
            return max_entry(_sal, cur, path_buf);
         }

         const auto branch_b = std::to_integer<std::uint8_t>(prefix[depth + plen]);
         auto       cit = std::lower_bound(in.children.begin(), in.children.end(), branch_b,
                                           [](const auto& pair, std::uint8_t b) { return pair.first < b; });
         if (cit == in.children.end() || cit->first != branch_b)
            return std::nullopt;

         path_buf.push_back(static_cast<std::byte>(cit->first));
         cur = cit->second;
         depth += plen + 1;
      }
      return std::nullopt;
   }

   std::uint64_t CowArt::count(const ArtRoot& root) const
   {
      return read_node_count(_sal, root.address());
   }

   std::uint64_t CowArt::count_range(const ArtRoot&             root,
                                     std::span<const std::byte> lower,
                                     std::span<const std::byte> upper) const
   {
      if (!upper.empty() && lex_compare(lower, upper) >= 0)
         return 0;
      std::vector<std::byte> path_buf;
      return count_range_rec(_sal, root.address(), path_buf, lower, upper);
   }

   ArtRoot CowArt::remove_range(const ArtRoot&             root,
                                std::span<const std::byte> lower,
                                std::span<const std::byte> upper)
   {
      if (!upper.empty() && lex_compare(lower, upper) >= 0)
         return root;
      std::vector<std::byte> path_buf;
      auto [new_addr, rem] = remove_range_rec(_sal, root.address(), path_buf, lower, upper);
      if (rem == 0)
         return root;
      return ArtRoot{&_sal, new_addr, /*adopt_existing_ref=*/true};
   }
}  // namespace muabase_db
