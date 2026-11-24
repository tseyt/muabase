#pragma once

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <span>
#include <vector>

namespace muabase_db::detail
{
   inline int lex_compare(std::span<const std::byte> a, std::span<const std::byte> b) noexcept
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

   inline bool starts_with(std::span<const std::byte> full,
                           std::span<const std::byte> prefix) noexcept
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

   inline std::vector<std::byte> key_successor(std::span<const std::byte> k)
   {
      std::vector<std::byte> out;
      out.reserve(k.size() + 1);
      out.insert(out.end(), k.begin(), k.end());
      out.push_back(std::byte{0});
      return out;
   }

   inline std::vector<std::byte> prefix_successor(std::span<const std::byte> pfx)
   {
      std::vector<std::byte> out(pfx.begin(), pfx.end());
      while (!out.empty() && std::to_integer<std::uint8_t>(out.back()) == 0xFFu)
      {
         out.pop_back();
      }
      if (!out.empty())
      {
         auto next  = static_cast<std::uint8_t>(std::to_integer<std::uint8_t>(out.back()) + 1u);
         out.back() = static_cast<std::byte>(next);
      }
      return out;
   }
}  // namespace muabase_db::detail
