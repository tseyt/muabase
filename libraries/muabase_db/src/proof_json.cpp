#include <muabase_db/dwal_merkle.hpp>

#include <atomic>
#include <cctype>
#include <cstdint>
#include <cstring>
#include <filesystem>
#include <string>
#include <string_view>
#include <unistd.h>
#include <utility>

namespace muabase_db
{
   std::string bytes_to_hex(std::span<const std::byte> data)
   {
      static constexpr char kHex[] = "0123456789abcdef";
      std::string           out;
      out.reserve(data.size() * 2);
      for (std::byte b : data)
      {
         auto v = std::to_integer<std::uint8_t>(b);
         out.push_back(kHex[v >> 4]);
         out.push_back(kHex[v & 0x0Fu]);
      }
      return out;
   }

   std::string checksum_to_hex(const Checksum256& digest)
   {
      return bytes_to_hex({reinterpret_cast<const std::byte*>(digest.data()), digest.size()});
   }

   std::optional<std::vector<std::byte>> hex_to_bytes(std::string_view hex)
   {
      if (hex.size() >= 2 && hex[0] == '0' && (hex[1] == 'x' || hex[1] == 'X'))
         hex.remove_prefix(2);
      if (hex.size() % 2 != 0)
         return std::nullopt;
      auto nibble = [](char c) -> int
      {
         if (c >= '0' && c <= '9')
            return c - '0';
         if (c >= 'a' && c <= 'f')
            return 10 + (c - 'a');
         if (c >= 'A' && c <= 'F')
            return 10 + (c - 'A');
         return -1;
      };
      std::vector<std::byte> out;
      out.reserve(hex.size() / 2);
      for (std::size_t i = 0; i < hex.size(); i += 2)
      {
         int hi = nibble(hex[i]);
         int lo = nibble(hex[i + 1]);
         if (hi < 0 || lo < 0)
            return std::nullopt;
         out.push_back(static_cast<std::byte>((hi << 4) | lo));
      }
      return out;
   }

   std::optional<Checksum256> hex_to_checksum(std::string_view hex)
   {
      auto bytes = hex_to_bytes(hex);
      if (!bytes || bytes->size() != 32)
         return std::nullopt;
      Checksum256 out{};
      std::memcpy(out.data(), bytes->data(), 32);
      return out;
   }

   namespace
   {
      void append_json_string(std::string& out, std::string_view s)
      {
         out.push_back('"');
         for (unsigned char c : s)
         {
            switch (c)
            {
               case '"':
                  out += "\\\"";
                  break;
               case '\\':
                  out += "\\\\";
                  break;
               case '\n':
                  out += "\\n";
                  break;
               case '\r':
                  out += "\\r";
                  break;
               case '\t':
                  out += "\\t";
                  break;
               default:
                  if (c < 0x20)
                  {
                     static constexpr char kHex[] = "0123456789abcdef";
                     out += "\\u00";
                     out.push_back(kHex[c >> 4]);
                     out.push_back(kHex[c & 0x0Fu]);
                  }
                  else
                  {
                     out.push_back(static_cast<char>(c));
                  }
                  break;
            }
         }
         out.push_back('"');
      }

      class MiniJsonParser
      {
        public:
         explicit MiniJsonParser(std::string_view src) : _s(src) {}

         void skip_ws()
         {
            while (_pos < _s.size() && std::isspace(static_cast<unsigned char>(_s[_pos])))
               ++_pos;
         }

         bool consume(char c)
         {
            skip_ws();
            if (_pos < _s.size() && _s[_pos] == c)
            {
               ++_pos;
               return true;
            }
            return false;
         }

         bool eof()
         {
            skip_ws();
            return _pos == _s.size();
         }

         std::optional<std::string> parse_string()
         {
            if (!consume('"'))
               return std::nullopt;
            std::string out;
            while (_pos < _s.size())
            {
               unsigned char uc = static_cast<unsigned char>(_s[_pos++]);
               if (uc == '"')
                  return out;
               if (uc < 0x20)
                  return std::nullopt;
               if (uc == '\\')
               {
                  if (_pos >= _s.size())
                     return std::nullopt;
                  char esc = _s[_pos++];
                  switch (esc)
                  {
                     case '"':
                        out.push_back('"');
                        break;
                     case '\\':
                        out.push_back('\\');
                        break;
                     case '/':
                        out.push_back('/');
                        break;
                     case 'b':
                        out.push_back('\b');
                        break;
                     case 'f':
                        out.push_back('\f');
                        break;
                     case 'n':
                        out.push_back('\n');
                        break;
                     case 'r':
                        out.push_back('\r');
                        break;
                     case 't':
                        out.push_back('\t');
                        break;
                     case 'u':
                     {
                        if (_pos + 4 > _s.size())
                           return std::nullopt;
                        unsigned code = 0;
                        for (int i = 0; i < 4; ++i)
                        {
                           char h = _s[_pos++];
                           code <<= 4;
                           if (h >= '0' && h <= '9')
                              code |= static_cast<unsigned>(h - '0');
                           else if (h >= 'a' && h <= 'f')
                              code |= static_cast<unsigned>(10 + h - 'a');
                           else if (h >= 'A' && h <= 'F')
                              code |= static_cast<unsigned>(10 + h - 'A');
                           else
                              return std::nullopt;
                        }
                        if (code > 0x7Fu)
                           return std::nullopt;
                        out.push_back(static_cast<char>(code));
                        break;
                     }
                     default:
                        return std::nullopt;
                  }
               }
               else
               {
                  out.push_back(static_cast<char>(uc));
               }
            }
            return std::nullopt;
         }

         std::optional<bool> parse_bool()
         {
            skip_ws();
            if (_s.substr(_pos, 4) == "true")
            {
               _pos += 4;
               return true;
            }
            if (_s.substr(_pos, 5) == "false")
            {
               _pos += 5;
               return false;
            }
            return std::nullopt;
         }

         std::optional<std::uint64_t> parse_u64()
         {
            skip_ws();
            if (_pos >= _s.size() || !std::isdigit(static_cast<unsigned char>(_s[_pos])))
               return std::nullopt;
            if (_s[_pos] == '0' && _pos + 1 < _s.size() &&
                std::isdigit(static_cast<unsigned char>(_s[_pos + 1])))
            {
               return std::nullopt;
            }
            std::uint64_t val = 0;
            while (_pos < _s.size() && std::isdigit(static_cast<unsigned char>(_s[_pos])))
            {
               std::uint64_t digit = static_cast<std::uint64_t>(_s[_pos++] - '0');
               if (val > (UINT64_MAX - digit) / 10)
                  return std::nullopt;
               val = val * 10 + digit;
            }
            return val;
         }

         template <typename Fn>
         bool parse_object(Fn&& on_field)
         {
            if (!consume('{'))
               return false;
            if (consume('}'))
               return true;
            while (true)
            {
               auto key = parse_string();
               if (!key || !consume(':'))
                  return false;
               if (!on_field(*key))
                  return false;
               if (consume('}'))
                  return true;
               if (!consume(','))
                  return false;
            }
         }

         template <typename Fn>
         bool parse_array(Fn&& on_elem)
         {
            if (!consume('['))
               return false;
            if (consume(']'))
               return true;
            while (true)
            {
               if (!on_elem())
                  return false;
               if (consume(']'))
                  return true;
               if (!consume(','))
                  return false;
            }
         }

        private:
         std::string_view _s;
         std::size_t      _pos = 0;
      };
   }  // namespace

   std::string anchored_proof_to_json(const AnchoredMerkleProof& bundle)
   {
      std::string out;
      out.reserve(512);
      out += "{\"version\":";
      append_json_string(out, bundle.version);
      out += ",\"anchor\":{\"blockNum\":";
      out += std::to_string(bundle.anchor.block_num);
      out += ",\"blockId\":";
      append_json_string(out, bundle.anchor.block_id);
      out += ",\"producer\":";
      append_json_string(out, bundle.anchor.producer);
      out += ",\"stateRoot\":";
      append_json_string(out, checksum_to_hex(bundle.anchor.state_root));
      out += ",\"headerSignature\":";
      append_json_string(out, bundle.anchor.header_signature);
      out += "},\"valueTag\":";
      out += std::to_string(static_cast<std::uint32_t>(bundle.value_tag));
      out += ",\"valueHex\":";
      append_json_string(out, bytes_to_hex(bundle.value));
      out += ",\"proof\":{\"isInclusion\":";
      out += bundle.proof.is_inclusion ? "true" : "false";
      out += ",\"emptyTree\":";
      out += bundle.proof.empty_tree ? "true" : "false";
      out += ",\"singleEntry\":";
      out += bundle.proof.single_entry ? "true" : "false";
      out += ",\"targetKeyHex\":";
      append_json_string(out, bytes_to_hex(bundle.proof.target_key));
      out += ",\"rootPrefixHex\":";
      append_json_string(out, bytes_to_hex(bundle.proof.root_prefix));
      out += ",\"witnessKeyHex\":";
      append_json_string(out, bytes_to_hex(bundle.proof.witness_key));
      out += ",\"witnessCoreHash\":";
      append_json_string(out, checksum_to_hex(bundle.proof.witness_core_hash));
      out += ",\"steps\":[";
      for (std::size_t i = 0; i < bundle.proof.steps.size(); ++i)
      {
         if (i > 0)
            out.push_back(',');
         const auto& step = bundle.proof.steps[i];
         out += "{\"edgePrefixHex\":";
         append_json_string(out, bytes_to_hex(step.edge_prefix));
         out += ",\"hasTerminal\":";
         out += step.has_terminal ? "true" : "false";
         out += ",\"terminalHash\":";
         append_json_string(out, checksum_to_hex(step.terminal_hash));
         out += ",\"siblings\":[";
         for (std::size_t j = 0; j < step.siblings.size(); ++j)
         {
            if (j > 0)
               out.push_back(',');
            const auto& sib = step.siblings[j];
            out += "{\"branchByte\":";
            out += std::to_string(static_cast<std::uint32_t>(sib.branch_byte));
            out += ",\"isLeaf\":";
            out += sib.is_leaf ? "true" : "false";
            out += ",\"edgePrefixHex\":";
            append_json_string(out, bytes_to_hex(sib.edge_prefix));
            out += ",\"coreHash\":";
            append_json_string(out, checksum_to_hex(sib.core_hash));
            out.push_back('}');
         }
         out += "]}";
      }
      out += "]}}";
      return out;
   }

   std::optional<AnchoredMerkleProof> anchored_proof_from_json(std::string_view json)
   {
      MiniJsonParser      p{json};
      AnchoredMerkleProof bundle;

      bool has_version   = false;
      bool has_anchor    = false;
      bool has_proof     = false;
      bool has_state_root = false;
      bool has_is_inc    = false;

      auto parse_hex_vec = [&](std::vector<std::byte>& dst) -> bool
      {
         auto s = p.parse_string();
         if (!s)
            return false;
         auto b = hex_to_bytes(*s);
         if (!b)
            return false;
         dst = std::move(*b);
         return true;
      };

      auto parse_checksum = [&](Checksum256& dst) -> bool
      {
         auto s = p.parse_string();
         if (!s)
            return false;
         auto c = hex_to_checksum(*s);
         if (!c)
            return false;
         dst = *c;
         return true;
      };

      auto parse_anchor = [&]() -> bool
      {
         return p.parse_object(
             [&](const std::string& k) -> bool
             {
                if (k == "blockNum")
                {
                   auto n = p.parse_u64();
                   if (!n || *n > UINT32_MAX)
                      return false;
                   bundle.anchor.block_num = static_cast<std::uint32_t>(*n);
                   return true;
                }
                if (k == "blockId")
                {
                   auto s = p.parse_string();
                   if (!s)
                      return false;
                   bundle.anchor.block_id = std::move(*s);
                   return true;
                }
                if (k == "producer")
                {
                   auto s = p.parse_string();
                   if (!s)
                      return false;
                   bundle.anchor.producer = std::move(*s);
                   return true;
                }
                if (k == "stateRoot")
                {
                   has_state_root = parse_checksum(bundle.anchor.state_root);
                   return has_state_root;
                }
                if (k == "headerSignature")
                {
                   auto s = p.parse_string();
                   if (!s)
                      return false;
                   bundle.anchor.header_signature = std::move(*s);
                   return true;
                }
                return false;
             });
      };

      auto parse_sibling = [&]() -> std::optional<MerkleSibling>
      {
         MerkleSibling sib;
         bool          ok = p.parse_object(
             [&](const std::string& k) -> bool
             {
                if (k == "branchByte")
                {
                   auto n = p.parse_u64();
                   if (!n || *n > 255)
                      return false;
                   sib.branch_byte = static_cast<std::uint8_t>(*n);
                   return true;
                }
                if (k == "isLeaf")
                {
                   auto b = p.parse_bool();
                   if (!b)
                      return false;
                   sib.is_leaf = *b;
                   return true;
                }
                if (k == "edgePrefixHex")
                   return parse_hex_vec(sib.edge_prefix);
                if (k == "coreHash")
                   return parse_checksum(sib.core_hash);
                return false;
             });
         if (!ok)
            return std::nullopt;
         return sib;
      };

      auto parse_step = [&]() -> std::optional<MerkleProofStep>
      {
         MerkleProofStep step;
         bool            ok = p.parse_object(
             [&](const std::string& k) -> bool
             {
                if (k == "edgePrefixHex")
                   return parse_hex_vec(step.edge_prefix);
                if (k == "hasTerminal")
                {
                   auto b = p.parse_bool();
                   if (!b)
                      return false;
                   step.has_terminal = *b;
                   return true;
                }
                if (k == "terminalHash")
                   return parse_checksum(step.terminal_hash);
                if (k == "siblings")
                {
                   return p.parse_array(
                       [&]() -> bool
                       {
                          auto sib = parse_sibling();
                          if (!sib)
                             return false;
                          step.siblings.push_back(std::move(*sib));
                          return true;
                       });
                }
                return false;
             });
         if (!ok)
            return std::nullopt;
         return step;
      };

      auto parse_proof = [&]() -> bool
      {
         return p.parse_object(
             [&](const std::string& k) -> bool
             {
                if (k == "isInclusion")
                {
                   auto b = p.parse_bool();
                   if (!b)
                      return false;
                   bundle.proof.is_inclusion = *b;
                   has_is_inc                = true;
                   return true;
                }
                if (k == "emptyTree")
                {
                   auto b = p.parse_bool();
                   if (!b)
                      return false;
                   bundle.proof.empty_tree = *b;
                   return true;
                }
                if (k == "singleEntry")
                {
                   auto b = p.parse_bool();
                   if (!b)
                      return false;
                   bundle.proof.single_entry = *b;
                   return true;
                }
                if (k == "targetKeyHex")
                   return parse_hex_vec(bundle.proof.target_key);
                if (k == "rootPrefixHex")
                   return parse_hex_vec(bundle.proof.root_prefix);
                if (k == "witnessKeyHex")
                   return parse_hex_vec(bundle.proof.witness_key);
                if (k == "witnessCoreHash")
                   return parse_checksum(bundle.proof.witness_core_hash);
                if (k == "steps")
                {
                   return p.parse_array(
                       [&]() -> bool
                       {
                          auto step = parse_step();
                          if (!step)
                             return false;
                          bundle.proof.steps.push_back(std::move(*step));
                          return true;
                       });
                }
                return false;
             });
      };

      bool ok = p.parse_object(
          [&](const std::string& k) -> bool
          {
             if (k == "version")
             {
                auto s = p.parse_string();
                if (!s)
                   return false;
                bundle.version = std::move(*s);
                has_version    = true;
                return true;
             }
             if (k == "anchor")
             {
                has_anchor = parse_anchor();
                return has_anchor;
             }
             if (k == "valueTag")
             {
                auto n = p.parse_u64();
                if (!n || *n > 1)
                   return false;
                bundle.value_tag = static_cast<ValueTag>(*n);
                return true;
             }
             if (k == "valueHex")
                return parse_hex_vec(bundle.value);
             if (k == "proof")
             {
                has_proof = parse_proof();
                return has_proof;
             }
             return false;
          });

      if (!ok || !p.eof() || !has_version || !has_anchor || !has_proof || !has_state_root ||
          !has_is_inc || bundle.version != kAnchoredProofVersion)
      {
         return std::nullopt;
      }
      return bundle;
   }

   bool verify_anchored_proof(const AnchoredMerkleProof& bundle)
   {
      if (bundle.version != kAnchoredProofVersion)
         return false;
      if (bundle.proof.is_inclusion)
      {
         return Merkleizer::verify_inclusion(bundle.anchor.state_root, bundle.proof.target_key,
                                             bundle.value, bundle.proof, bundle.value_tag);
      }
      return Merkleizer::verify_non_inclusion(bundle.anchor.state_root, bundle.proof.target_key,
                                              bundle.proof);
   }

   // =========================================================================
   // EphemeralProofBuilder (non-leaky snapshot proof builder)
   // =========================================================================

   struct EphemeralProofBuilder::Impl
   {
      struct TempDirGuard
      {
         std::filesystem::path dir;

         TempDirGuard()
         {
            static std::atomic<std::uint64_t> seq{0};
            dir = std::filesystem::temp_directory_path() /
                  ("muabase_proof_" + std::to_string(::getpid()) + "_" + std::to_string(++seq));
            std::filesystem::create_directories(dir);
         }

         ~TempDirGuard()
         {
            std::error_code ec;
            std::filesystem::remove_all(dir, ec);
         }
      };

      TempDirGuard     tmp;
      SegmentAllocator sal;
      CowArt           art;
      ArtRoot          root;
      Merkleizer       merkle;

      explicit Impl(std::size_t segment_bytes)
          : tmp(),
            sal(tmp.dir, SalConfig{.segment_bytes = segment_bytes}),
            art(sal),
            root(),
            merkle(art)
      {
      }
   };

   EphemeralProofBuilder::EphemeralProofBuilder(std::size_t segment_bytes)
       : _impl(std::make_unique<Impl>(segment_bytes))
   {
   }

   EphemeralProofBuilder::~EphemeralProofBuilder() = default;

   void EphemeralProofBuilder::upsert(std::span<const std::byte> key,
                                      std::span<const std::byte> value,
                                      ValueTag                   tag)
   {
      _impl->root = _impl->art.upsert(_impl->root, key, value, tag);
   }

   Checksum256 EphemeralProofBuilder::state_root()
   {
      return _impl->merkle.compute_root(_impl->root);
   }

   AnchoredMerkleProof EphemeralProofBuilder::prove(std::span<const std::byte> target_key,
                                                    BlockHeaderAnchor          anchor)
   {
      AnchoredMerkleProof bundle;
      anchor.state_root = _impl->merkle.compute_root(_impl->root);
      bundle.anchor     = std::move(anchor);

      ValueTag found_tag = ValueTag::Bytes;
      if (auto found = _impl->art.get(_impl->root, target_key, &found_tag))
      {
         bundle.value_tag = found_tag;
         bundle.value     = std::move(*found);
         bundle.proof     = _impl->merkle.prove_inclusion(_impl->root, target_key);
      }
      else
      {
         bundle.proof = _impl->merkle.prove_non_inclusion(_impl->root, target_key);
      }
      return bundle;
   }
}  // namespace muabase_db
