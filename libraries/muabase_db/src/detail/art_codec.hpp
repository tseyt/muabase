#pragma once

#include <muabase_db/cow_art.hpp>

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <span>
#include <stdexcept>
#include <utility>
#include <vector>

namespace muabase_db::detail
{
   inline constexpr std::size_t kLeafHeaderBytes      = 4;
   inline constexpr std::size_t kLeafEntryHeaderBytes = 8;
   inline constexpr std::size_t kInnerHeaderBytes     = 20;

   inline PtrAddress decode_subtree_addr(std::span<const std::byte> val)
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
      std::uint64_t                                    subtree_count = 0;
      std::vector<std::byte>                           prefix;
      bool                                             has_terminal = false;
      ValueTag                                         terminal_tag = ValueTag::Bytes;
      std::vector<std::byte>                           terminal_val;
      std::vector<std::pair<std::uint8_t, PtrAddress>> children;  // sorted ascending by byte
   };

   inline DecodedLeaf decode_leaf(std::span<const std::byte> payload)
   {
      if (payload.size() < kLeafHeaderBytes)
         throw std::runtime_error("Corrupt BTreeLeaf header");
      std::uint16_t cnt = 0;
      std::memcpy(&cnt, payload.data(), 2);

      DecodedLeaf out;
      out.entries.reserve(cnt);
      std::size_t off = kLeafHeaderBytes;
      for (std::uint16_t i = 0; i < cnt; ++i)
      {
         if (payload.size() - off < kLeafEntryHeaderBytes)
            throw std::runtime_error("Corrupt BTreeLeaf entry header");
         std::uint16_t klen = 0;
         std::uint32_t vlen = 0;
         std::memcpy(&klen, payload.data() + off, 2);
         auto tag = static_cast<ValueTag>(std::to_integer<std::uint8_t>(payload[off + 2]));
         std::memcpy(&vlen, payload.data() + off + 4, 4);
         off += kLeafEntryHeaderBytes;
         const std::uint64_t need = static_cast<std::uint64_t>(klen) + vlen;
         if (need > payload.size() - off)
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
   //   [16..20) u32 terminal_len
   inline DecodedInner decode_inner(std::span<const std::byte> payload)
   {
      if (payload.size() < kInnerHeaderBytes)
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

      const std::uint64_t unaligned =
          static_cast<std::uint64_t>(kInnerHeaderBytes) + plen + tlen + nc;
      const std::uint64_t aligned = (unaligned + 3u) & ~std::uint64_t{3u};
      const std::uint64_t total   = aligned + static_cast<std::uint64_t>(nc) * sizeof(std::uint32_t);
      if (payload.size() < total)
         throw std::runtime_error("Corrupt ArtInner body");

      std::size_t off = kInnerHeaderBytes;
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
}  // namespace muabase_db::detail
