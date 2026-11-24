#include <muabase_db/dwal_merkle.hpp>

#include <openssl/evp.h>

#include <algorithm>
#include <array>
#include <cctype>
#include <cerrno>
#include <cstring>
#include <fcntl.h>
#include <functional>
#include <map>
#include <mutex>
#include <shared_mutex>
#include <stdexcept>
#include <unistd.h>

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

      // Standard IEEE 802.3 CRC32 for WAL frame integrity
      std::uint32_t crc32_ieee(std::span<const std::byte> data) noexcept
      {
         std::uint32_t crc = 0xFFFFFFFFu;
         for (std::byte b : data)
         {
            crc ^= std::to_integer<std::uint8_t>(b);
            for (int bit = 0; bit < 8; ++bit)
            {
               const std::uint32_t mask = -(crc & 1u);
               crc                      = (crc >> 1) ^ (0xEDB88320u & mask);
            }
         }
         return ~crc;
      }

      enum class WalOp : std::uint8_t
      {
         Upsert          = 1,
         Remove          = 2,
         FlushCheckpoint = 3,
      };

      constexpr std::uint32_t kWalFrameMagic = 0x4C41574Du;  // "MWAL"

      void write_all_fd(int fd, const void* buf, std::size_t len)
      {
         const auto* p   = static_cast<const std::uint8_t*>(buf);
         std::size_t rem = len;
         while (rem > 0)
         {
            ssize_t w = ::write(fd, p, rem);
            if (w < 0)
            {
               if (errno == EINTR)
                  continue;
               throw std::runtime_error("DWAL write failed");
            }
            p += static_cast<std::size_t>(w);
            rem -= static_cast<std::size_t>(w);
         }
      }

      struct OverlayValue
      {
         bool                   tombstone = false;
         ValueTag               tag       = ValueTag::Bytes;
         std::vector<std::byte> value     = {};
      };

      using OverlayMap = std::map<std::vector<std::byte>, OverlayValue, ByteLess>;

      struct RootSlot
      {
         mutable std::shared_mutex mtx;
         ArtRoot                   base;
         OverlayMap                rw;
         OverlayMap                ro;
         std::uint64_t             rw_epoch       = 1;
         std::uint64_t             ro_epoch       = 0;
         std::uint64_t             flushed_epochs = 0;
         std::uint64_t             wal_replayed   = 0;
      };

      std::vector<std::byte> key_successor(std::span<const std::byte> k)
      {
         std::vector<std::byte> out;
         out.reserve(k.size() + 1);
         out.insert(out.end(), k.begin(), k.end());
         out.push_back(std::byte{0});
         return out;
      }

      std::vector<std::byte> prefix_successor(std::span<const std::byte> pfx)
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

      // --- SHA-256 Digest Helpers ---
      class Sha256Hasher
      {
        public:
         Sha256Hasher() : _ctx(::EVP_MD_CTX_new())
         {
            if (!_ctx || ::EVP_DigestInit_ex(_ctx, ::EVP_sha256(), nullptr) != 1)
               throw std::runtime_error("EVP_DigestInit_ex(SHA-256) failed");
         }
         ~Sha256Hasher()
         {
            if (_ctx)
               ::EVP_MD_CTX_free(_ctx);
         }
         Sha256Hasher(const Sha256Hasher&)            = delete;
         Sha256Hasher& operator=(const Sha256Hasher&) = delete;

         void update(const void* data, std::size_t len)
         {
            if (len > 0 && ::EVP_DigestUpdate(_ctx, data, len) != 1)
               throw std::runtime_error("EVP_DigestUpdate failed");
         }

         void update_u8(std::uint8_t v) { update(&v, 1); }
         void update_u16(std::uint16_t v)
         {
            const std::uint8_t le[2] = {
                static_cast<std::uint8_t>(v & 0xFFu),
                static_cast<std::uint8_t>((v >> 8) & 0xFFu),
            };
            update(le, sizeof(le));
         }
         void update_u32(std::uint32_t v)
         {
            const std::uint8_t le[4] = {
                static_cast<std::uint8_t>(v & 0xFFu),
                static_cast<std::uint8_t>((v >> 8) & 0xFFu),
                static_cast<std::uint8_t>((v >> 16) & 0xFFu),
                static_cast<std::uint8_t>((v >> 24) & 0xFFu),
            };
            update(le, sizeof(le));
         }

         Checksum256 finalize()
         {
            Checksum256  out{};
            unsigned int out_len = 0;
            if (::EVP_DigestFinal_ex(_ctx, out.data(), &out_len) != 1 || out_len != out.size())
               throw std::runtime_error("EVP_DigestFinal_ex failed");
            return out;
         }

        private:
         EVP_MD_CTX* _ctx = nullptr;
      };

      Checksum256 hash_split_core(bool                            has_terminal,
                                  const Checksum256&              terminal_hash,
                                  std::span<const MerkleSibling>  children)
      {
         Sha256Hasher h;
         h.update_u8(0x02u);
         h.update_u8(has_terminal ? 1u : 0u);
         if (has_terminal)
         {
            h.update(terminal_hash.data(), terminal_hash.size());
         }
         h.update_u16(static_cast<std::uint16_t>(children.size()));
         for (const auto& c : children)
         {
            h.update_u8(c.is_leaf ? 1u : 0u);
            h.update_u8(c.branch_byte);
            h.update_u16(static_cast<std::uint16_t>(c.edge_prefix.size()));
            if (!c.edge_prefix.empty())
               h.update(c.edge_prefix.data(), c.edge_prefix.size());
            h.update(c.core_hash.data(), c.core_hash.size());
         }
         return h.finalize();
      }

      Checksum256 hash_root_envelope(std::span<const std::byte> root_prefix,
                                     const Checksum256&         core_hash)
      {
         Sha256Hasher h;
         h.update_u8(0x03u);
         h.update_u16(static_cast<std::uint16_t>(root_prefix.size()));
         if (!root_prefix.empty())
            h.update(root_prefix.data(), root_prefix.size());
         h.update(core_hash.data(), core_hash.size());
         return h.finalize();
      }

      std::uint64_t fnv1a_bytes(std::span<const std::byte> data) noexcept
      {
         std::uint64_t h = 14695981039346656037ull;
         for (std::byte b : data)
         {
            h ^= std::to_integer<std::uint8_t>(b);
            h *= 1099511628211ull;
         }
         return h;
      }

      // Wire decoders matching CowArt's BTreeLeaf and ArtInner node payloads
      std::vector<KvEntry> decode_leaf_payload(std::span<const std::byte> payload)
      {
         if (payload.size() < 4)
            throw std::runtime_error("Corrupt BTreeLeaf in Merkleizer");
         std::uint16_t cnt = 0;
         std::memcpy(&cnt, payload.data(), 2);

         std::vector<KvEntry> entries;
         entries.reserve(cnt);
         std::size_t off = 4;
         for (std::uint16_t i = 0; i < cnt; ++i)
         {
            if (off + 8 > payload.size())
               throw std::runtime_error("Corrupt BTreeLeaf entry header in Merkleizer");
            std::uint16_t klen = 0;
            std::uint32_t vlen = 0;
            std::memcpy(&klen, payload.data() + off, 2);
            auto tag = static_cast<ValueTag>(std::to_integer<std::uint8_t>(payload[off + 2]));
            std::memcpy(&vlen, payload.data() + off + 4, 4);
            off += 8;
            if (off + klen + vlen > payload.size())
               throw std::runtime_error("Corrupt BTreeLeaf entry body in Merkleizer");

            KvEntry e;
            e.tag = tag;
            e.key.assign(payload.begin() + static_cast<std::ptrdiff_t>(off),
                         payload.begin() + static_cast<std::ptrdiff_t>(off + klen));
            off += klen;
            e.value.assign(payload.begin() + static_cast<std::ptrdiff_t>(off),
                           payload.begin() + static_cast<std::ptrdiff_t>(off + vlen));
            off += vlen;
            entries.push_back(std::move(e));
         }
         return entries;
      }

      struct InnerView
      {
         std::uint64_t                                    subtree_count = 0;
         std::vector<std::byte>                           prefix;
         bool                                             has_terminal = false;
         ValueTag                                         terminal_tag = ValueTag::Bytes;
         std::vector<std::byte>                           terminal_val;
         std::vector<std::pair<std::uint8_t, PtrAddress>> children;
      };

      InnerView decode_inner_payload(std::span<const std::byte> payload)
      {
         if (payload.size() < 20)
            throw std::runtime_error("Corrupt ArtInner header in Merkleizer");

         InnerView     out;
         std::uint16_t plen = 0;
         std::uint16_t nc   = 0;
         std::uint32_t tlen = 0;
         std::memcpy(&out.subtree_count, payload.data(), 8);
         std::memcpy(&plen, payload.data() + 8, 2);
         std::memcpy(&nc, payload.data() + 10, 2);
         out.has_terminal = std::to_integer<std::uint8_t>(payload[12]) != 0;
         out.terminal_tag = static_cast<ValueTag>(std::to_integer<std::uint8_t>(payload[13]));
         std::memcpy(&tlen, payload.data() + 16, 4);

         const std::size_t unaligned = 20u + plen + tlen + nc;
         const std::size_t aligned   = (unaligned + 3u) & ~std::size_t{3u};
         const std::size_t total     = aligned + static_cast<std::size_t>(nc) * sizeof(std::uint32_t);
         if (payload.size() < total)
            throw std::runtime_error("Corrupt ArtInner body in Merkleizer");

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
            std::memcpy(&raw_addr, payload.data() + aligned + i * sizeof(std::uint32_t), 4);
            out.children.emplace_back(b, PtrAddress{raw_addr});
         }
         return out;
      }

      // Canonical Patricia node expansion used for generating inclusion & non-inclusion proofs
      struct CanonicalNode
      {
         std::uint64_t              count = 0;
         std::vector<std::byte>     edge_prefix;
         Checksum256                core_hash{};
         // Populated if count == 1:
         std::vector<std::byte>     single_full_key;
         // Populated if count >= 2:
         bool                       has_terminal = false;
         Checksum256                terminal_hash{};
         std::vector<MerkleSibling> children;
      };
   }  // namespace

   // =========================================================================
   // DwalEngine
   // =========================================================================

   struct DwalEngine::Impl
   {
      std::filesystem::path                 wal_dir;
      std::filesystem::path                 wal_path;
      CowArt&                               art;
      DwalConfig                            cfg;
      int                                   wal_fd = -1;
      mutable std::mutex                    wal_mtx;
      std::array<RootSlot, kMaxRoots>       roots;

      Impl(const std::filesystem::path& dir, CowArt& a, DwalConfig c)
          : wal_dir(dir), wal_path(dir / "dwal.log"), art(a), cfg(c)
      {
         std::filesystem::create_directories(wal_dir);
         wal_fd = ::open(wal_path.c_str(), O_CREAT | O_RDWR, 0644);
         if (wal_fd < 0)
            throw std::runtime_error("Failed to open dwal.log");
         replay_wal();
      }

      ~Impl()
      {
         if (wal_fd >= 0)
            ::close(wal_fd);
      }

      void check_root(std::uint16_t root_id) const
      {
         if (root_id >= kMaxRoots)
            throw std::out_of_range("DWAL root_id exceeds kMaxRoots");
      }

      void append_wal_frame(WalOp                      op,
                            ValueTag                   tag,
                            std::uint16_t              root_id,
                            std::uint64_t              epoch,
                            std::span<const std::byte> key,
                            std::span<const std::byte> val)
      {
         const auto        klen     = static_cast<std::uint32_t>(key.size());
         const auto        vlen     = static_cast<std::uint32_t>(val.size());
         const std::size_t body_len = 20u + klen + vlen;

         std::vector<std::byte> frame(12u + body_len);
         std::byte*             body = frame.data() + 12;

         body[0] = static_cast<std::byte>(op);
         body[1] = static_cast<std::byte>(tag);
         std::memcpy(body + 2, &root_id, 2);
         std::memcpy(body + 4, &epoch, 8);
         std::memcpy(body + 12, &klen, 4);
         std::memcpy(body + 16, &vlen, 4);
         if (klen > 0)
            std::memcpy(body + 20, key.data(), klen);
         if (vlen > 0)
            std::memcpy(body + 20 + klen, val.data(), vlen);

         const std::uint32_t magic = kWalFrameMagic;
         const auto          blen  = static_cast<std::uint32_t>(body_len);
         const std::uint32_t crc   = crc32_ieee({body, body_len});

         std::memcpy(frame.data() + 0, &magic, 4);
         std::memcpy(frame.data() + 4, &blen, 4);
         std::memcpy(frame.data() + 8, &crc, 4);

         std::lock_guard lk{wal_mtx};
         write_all_fd(wal_fd, frame.data(), frame.size());
         if (cfg.sync_wal_on_write)
            ::fdatasync(wal_fd);
      }

      void replay_wal()
      {
         const off_t end_off = ::lseek(wal_fd, 0, SEEK_END);
         if (end_off <= 0)
            return;

         std::vector<std::byte> file_buf(static_cast<std::size_t>(end_off));
         if (::pread(wal_fd, file_buf.data(), file_buf.size(), 0) != end_off)
            throw std::runtime_error("Failed reading dwal.log for recovery");

         struct ParsedRecord
         {
            WalOp                  op{};
            ValueTag               tag{};
            std::uint16_t          root_id = 0;
            std::uint64_t          epoch   = 0;
            std::vector<std::byte> key;
            std::vector<std::byte> val;
         };

         std::vector<ParsedRecord>           records;
         std::array<std::uint64_t, kMaxRoots> max_flushed{};
         std::array<std::uint64_t, kMaxRoots> max_seen{};

         std::size_t valid_off = 0;
         while (valid_off + 12 <= file_buf.size())
         {
            std::uint32_t magic    = 0;
            std::uint32_t body_len = 0;
            std::uint32_t crc      = 0;
            std::memcpy(&magic, file_buf.data() + valid_off, 4);
            std::memcpy(&body_len, file_buf.data() + valid_off + 4, 4);
            std::memcpy(&crc, file_buf.data() + valid_off + 8, 4);

            if (magic != kWalFrameMagic || body_len < 20 ||
                valid_off + 12u + body_len > file_buf.size())
            {
               break;  // Torn or corrupt tail frame
            }

            std::span<const std::byte> body{file_buf.data() + valid_off + 12, body_len};
            if (crc32_ieee(body) != crc)
            {
               break;  // Torn payload checksum mismatch
            }

            const auto    op      = static_cast<WalOp>(std::to_integer<std::uint8_t>(body[0]));
            const auto    tag     = static_cast<ValueTag>(std::to_integer<std::uint8_t>(body[1]));
            std::uint16_t root_id = 0;
            std::uint64_t epoch   = 0;
            std::uint32_t klen    = 0;
            std::uint32_t vlen    = 0;
            std::memcpy(&root_id, body.data() + 2, 2);
            std::memcpy(&epoch, body.data() + 4, 8);
            std::memcpy(&klen, body.data() + 12, 4);
            std::memcpy(&vlen, body.data() + 16, 4);

            if (root_id >= kMaxRoots || 20u + klen + vlen != body_len)
               break;

            max_seen[root_id] = std::max(max_seen[root_id], epoch);
            if (op == WalOp::FlushCheckpoint)
            {
               max_flushed[root_id] = std::max(max_flushed[root_id], epoch);
            }
            else
            {
               ParsedRecord rec;
               rec.op      = op;
               rec.tag     = tag;
               rec.root_id = root_id;
               rec.epoch   = epoch;
               rec.key.assign(body.begin() + 20,
                              body.begin() + static_cast<std::ptrdiff_t>(20u + klen));
               rec.val.assign(body.begin() + static_cast<std::ptrdiff_t>(20u + klen),
                              body.begin() + static_cast<std::ptrdiff_t>(20u + klen + vlen));
               records.push_back(std::move(rec));
            }
            valid_off += 12u + body_len;
         }

         if (valid_off < file_buf.size())
         {
            // Repair torn tail frame so future appends start cleanly on frame boundary
            if (::ftruncate(wal_fd, static_cast<off_t>(valid_off)) != 0)
               throw std::runtime_error("Failed truncating torn DWAL tail");
         }
         ::lseek(wal_fd, static_cast<off_t>(valid_off), SEEK_SET);

         for (std::uint16_t r = 0; r < kMaxRoots; ++r)
         {
            roots[r].flushed_epochs = max_flushed[r];
            roots[r].rw_epoch       = std::max(max_flushed[r], max_seen[r]) + 1;
         }

         for (auto& rec : records)
         {
            if (rec.epoch <= max_flushed[rec.root_id])
               continue;

            auto& slot = roots[rec.root_id];
            ++slot.wal_replayed;
            if (rec.op == WalOp::Upsert)
            {
               slot.rw[std::move(rec.key)] = OverlayValue{
                   .tombstone = false,
                   .tag       = rec.tag,
                   .value     = std::move(rec.val),
               };
            }
            else if (rec.op == WalOp::Remove)
            {
               slot.rw[std::move(rec.key)] = OverlayValue{.tombstone = true};
            }
         }
      }

      ArtRoot drain_ro_locked(std::uint16_t root_id, RootSlot& slot)
      {
         if (slot.ro.empty())
            return slot.base;

         ArtRoot cur = slot.base;
         for (const auto& [k, ov] : slot.ro)
         {
            if (ov.tombstone)
            {
               cur = art.remove(cur, k).first;
            }
            else
            {
               cur = art.upsert(cur, k, ov.value, ov.tag);
            }
         }
         slot.base = cur;
         slot.ro.clear();
         if (slot.ro_epoch > slot.flushed_epochs)
         {
            slot.flushed_epochs = slot.ro_epoch;
            append_wal_frame(WalOp::FlushCheckpoint, ValueTag::Bytes, root_id, slot.flushed_epochs,
                             {}, {});
         }
         slot.ro_epoch = 0;
         return slot.base;
      }
   };

   DwalEngine::DwalEngine(const std::filesystem::path& wal_dir, CowArt& art, DwalConfig cfg)
       : _impl(std::make_unique<Impl>(wal_dir, art, cfg))
   {
   }

   DwalEngine::~DwalEngine() = default;

   void DwalEngine::set_base_root(std::uint16_t root_id, ArtRoot base)
   {
      _impl->check_root(root_id);
      auto&           slot = _impl->roots[root_id];
      std::unique_lock lk{slot.mtx};
      slot.base = std::move(base);
   }

   ArtRoot DwalEngine::base_root(std::uint16_t root_id) const
   {
      _impl->check_root(root_id);
      const auto&     slot = _impl->roots[root_id];
      std::shared_lock lk{slot.mtx};
      return slot.base;
   }

   void DwalEngine::upsert(std::uint16_t              root_id,
                           std::span<const std::byte> key,
                           std::span<const std::byte> value,
                           ValueTag                   tag)
   {
      _impl->check_root(root_id);
      auto&            slot = _impl->roots[root_id];
      std::unique_lock lk{slot.mtx};

      _impl->append_wal_frame(WalOp::Upsert, tag, root_id, slot.rw_epoch, key, value);
      std::vector<std::byte> kvec(key.begin(), key.end());
      slot.rw[std::move(kvec)] = OverlayValue{
          .tombstone = false,
          .tag       = tag,
          .value     = {value.begin(), value.end()},
      };

      if (slot.rw.size() >= _impl->cfg.swap_threshold_ops && slot.ro.empty())
      {
         slot.ro       = std::move(slot.rw);
         slot.rw.clear();
         slot.ro_epoch = slot.rw_epoch++;
      }
   }

   void DwalEngine::remove(std::uint16_t root_id, std::span<const std::byte> key)
   {
      _impl->check_root(root_id);
      auto&            slot = _impl->roots[root_id];
      std::unique_lock lk{slot.mtx};

      _impl->append_wal_frame(WalOp::Remove, ValueTag::Bytes, root_id, slot.rw_epoch, key, {});
      std::vector<std::byte> kvec(key.begin(), key.end());
      slot.rw[std::move(kvec)] = OverlayValue{.tombstone = true};

      if (slot.rw.size() >= _impl->cfg.swap_threshold_ops && slot.ro.empty())
      {
         slot.ro       = std::move(slot.rw);
         slot.rw.clear();
         slot.ro_epoch = slot.rw_epoch++;
      }
   }

   bool DwalEngine::swap_rw_to_ro(std::uint16_t root_id)
   {
      _impl->check_root(root_id);
      auto&            slot = _impl->roots[root_id];
      std::unique_lock lk{slot.mtx};
      if (slot.rw.empty())
         return false;
      if (!slot.ro.empty())
      {
         _impl->drain_ro_locked(root_id, slot);
      }
      slot.ro       = std::move(slot.rw);
      slot.rw.clear();
      slot.ro_epoch = slot.rw_epoch++;
      return true;
   }

   ArtRoot DwalEngine::flush(std::uint16_t root_id, bool freeze_rw)
   {
      _impl->check_root(root_id);
      auto&            slot = _impl->roots[root_id];
      std::unique_lock lk{slot.mtx};

      if (!slot.ro.empty())
         _impl->drain_ro_locked(root_id, slot);

      if (freeze_rw && !slot.rw.empty())
      {
         slot.ro       = std::move(slot.rw);
         slot.rw.clear();
         slot.ro_epoch = slot.rw_epoch++;
         _impl->drain_ro_locked(root_id, slot);
      }
      return slot.base;
   }

   void DwalEngine::flush_all()
   {
      for (std::uint16_t r = 0; r < kMaxRoots; ++r)
      {
         flush(r, true);
      }
   }

   std::optional<std::vector<std::byte>> DwalEngine::get(std::uint16_t              root_id,
                                                         std::span<const std::byte> key,
                                                         ValueTag*                  out_tag) const
   {
      _impl->check_root(root_id);
      const auto&      slot = _impl->roots[root_id];
      std::shared_lock lk{slot.mtx};

      if (auto it = slot.rw.find(key); it != slot.rw.end())
      {
         if (it->second.tombstone)
            return std::nullopt;
         if (out_tag)
            *out_tag = it->second.tag;
         return it->second.value;
      }
      if (auto it = slot.ro.find(key); it != slot.ro.end())
      {
         if (it->second.tombstone)
            return std::nullopt;
         if (out_tag)
            *out_tag = it->second.tag;
         return it->second.value;
      }
      return _impl->art.get(slot.base, key, out_tag);
   }

   std::optional<KvEntry> DwalEngine::lower_bound(std::uint16_t              root_id,
                                                  std::span<const std::byte> key,
                                                  std::size_t                prefix_len) const
   {
      _impl->check_root(root_id);
      const auto&      slot = _impl->roots[root_id];
      std::shared_lock lk{slot.mtx};

      const std::size_t          clamped = std::min(prefix_len, key.size());
      std::span<const std::byte> pfx     = prefix_len > 0 ? key.subspan(0, clamped) : std::span<const std::byte>{};

      auto in_pfx = [&](std::span<const std::byte> cand_k) -> bool
      { return prefix_len == 0 || starts_with(cand_k, pfx); };

      std::optional<KvEntry> best;
      auto                   consider = [&](KvEntry&& cand)
      {
         if (!best.has_value() || lex_compare(cand.key, best->key) < 0)
            best = std::move(cand);
      };

      // 1. Active RW overlay
      for (auto it = slot.rw.lower_bound(key); it != slot.rw.end(); ++it)
      {
         if (!in_pfx(it->first))
            break;
         if (!it->second.tombstone)
         {
            consider(KvEntry{.key = it->first, .value = it->second.value, .tag = it->second.tag});
            break;
         }
      }

      // 2. Frozen RO overlay (ignoring keys shadowed in RW)
      for (auto it = slot.ro.lower_bound(key); it != slot.ro.end(); ++it)
      {
         if (!in_pfx(it->first))
            break;
         if (slot.rw.find(it->first) != slot.rw.end())
            continue;
         if (!it->second.tombstone)
         {
            consider(KvEntry{.key = it->first, .value = it->second.value, .tag = it->second.tag});
            break;
         }
      }

      // 3. Persistent CowArt base (advancing past keys shadowed in RW or RO)
      std::vector<std::byte> probe(key.begin(), key.end());
      while (true)
      {
         auto cand = _impl->art.lower_bound(slot.base, probe, prefix_len);
         if (!cand.has_value())
            break;
         if (slot.rw.find(cand->key) == slot.rw.end() && slot.ro.find(cand->key) == slot.ro.end())
         {
            consider(std::move(*cand));
            break;
         }
         probe = key_successor(cand->key);
      }

      return best;
   }

   std::optional<KvEntry> DwalEngine::less_than(std::uint16_t              root_id,
                                                std::span<const std::byte> key,
                                                std::size_t                prefix_len) const
   {
      _impl->check_root(root_id);
      const auto&      slot = _impl->roots[root_id];
      std::shared_lock lk{slot.mtx};

      const std::size_t          clamped = std::min(prefix_len, key.size());
      std::span<const std::byte> pfx     = prefix_len > 0 ? key.subspan(0, clamped) : std::span<const std::byte>{};

      auto in_pfx = [&](std::span<const std::byte> cand_k) -> bool
      { return prefix_len == 0 || starts_with(cand_k, pfx); };

      std::optional<KvEntry> best;
      auto                   consider = [&](KvEntry&& cand)
      {
         if (!best.has_value() || lex_compare(cand.key, best->key) > 0)
            best = std::move(cand);
      };

      // 1. Active RW overlay
      {
         auto it = slot.rw.lower_bound(key);
         while (it != slot.rw.begin())
         {
            --it;
            if (!in_pfx(it->first))
               break;
            if (!it->second.tombstone)
            {
               consider(KvEntry{.key = it->first, .value = it->second.value, .tag = it->second.tag});
               break;
            }
         }
      }

      // 2. Frozen RO overlay
      {
         auto it = slot.ro.lower_bound(key);
         while (it != slot.ro.begin())
         {
            --it;
            if (!in_pfx(it->first))
               break;
            if (slot.rw.find(it->first) != slot.rw.end())
               continue;
            if (!it->second.tombstone)
            {
               consider(KvEntry{.key = it->first, .value = it->second.value, .tag = it->second.tag});
               break;
            }
         }
      }

      // 3. Persistent CowArt base
      std::vector<std::byte> probe(key.begin(), key.end());
      while (true)
      {
         auto cand = _impl->art.less_than(slot.base, probe, prefix_len);
         if (!cand.has_value())
            break;
         if (slot.rw.find(cand->key) == slot.rw.end() && slot.ro.find(cand->key) == slot.ro.end())
         {
            consider(std::move(*cand));
            break;
         }
         probe = std::move(cand->key);
      }

      return best;
   }

   std::optional<KvEntry> DwalEngine::max_with_prefix(std::uint16_t              root_id,
                                                      std::span<const std::byte> prefix) const
   {
      _impl->check_root(root_id);
      const auto&      slot = _impl->roots[root_id];
      std::shared_lock lk{slot.mtx};

      std::vector<std::byte> upper = prefix_successor(prefix);

      std::optional<KvEntry> best;
      auto                   consider = [&](KvEntry&& cand)
      {
         if (!best.has_value() || lex_compare(cand.key, best->key) > 0)
            best = std::move(cand);
      };

      // 1. Active RW overlay
      {
         auto it = upper.empty() ? slot.rw.end() : slot.rw.lower_bound(upper);
         while (it != slot.rw.begin())
         {
            --it;
            if (!starts_with(it->first, prefix))
               break;
            if (!it->second.tombstone)
            {
               consider(KvEntry{.key = it->first, .value = it->second.value, .tag = it->second.tag});
               break;
            }
         }
      }

      // 2. Frozen RO overlay
      {
         auto it = upper.empty() ? slot.ro.end() : slot.ro.lower_bound(upper);
         while (it != slot.ro.begin())
         {
            --it;
            if (!starts_with(it->first, prefix))
               break;
            if (slot.rw.find(it->first) != slot.rw.end())
               continue;
            if (!it->second.tombstone)
            {
               consider(KvEntry{.key = it->first, .value = it->second.value, .tag = it->second.tag});
               break;
            }
         }
      }

      // 3. Persistent CowArt base
      auto cand = _impl->art.max_with_prefix(slot.base, prefix);
      while (cand.has_value())
      {
         if (slot.rw.find(cand->key) == slot.rw.end() && slot.ro.find(cand->key) == slot.ro.end())
         {
            consider(std::move(*cand));
            break;
         }
         std::vector<std::byte> prev_probe = std::move(cand->key);
         cand                              = _impl->art.less_than(slot.base, prev_probe, 0);
         if (cand.has_value() && !starts_with(cand->key, prefix))
            break;
      }

      return best;
   }

   DwalRootStats DwalEngine::root_stats(std::uint16_t root_id) const
   {
      _impl->check_root(root_id);
      const auto&      slot = _impl->roots[root_id];
      std::shared_lock lk{slot.mtx};
      return DwalRootStats{
          .rw_entries     = slot.rw.size(),
          .ro_entries     = slot.ro.size(),
          .flushed_epochs = slot.flushed_epochs,
          .wal_replayed   = slot.wal_replayed,
      };
   }

   std::filesystem::path DwalEngine::wal_file_path() const
   {
      return _impl->wal_path;
   }

   // =========================================================================
   // Merkleizer
   // =========================================================================

   Merkleizer::Merkleizer(CowArt& art) : _art(art) {}

   Checksum256 Merkleizer::hash_entry(std::span<const std::byte> key,
                                      ValueTag                   tag,
                                      std::span<const std::byte> value)
   {
      Sha256Hasher h;
      h.update_u8(0x01u);
      h.update_u32(static_cast<std::uint32_t>(key.size()));
      if (!key.empty())
         h.update(key.data(), key.size());
      h.update_u8(static_cast<std::uint8_t>(tag));
      h.update_u32(static_cast<std::uint32_t>(value.size()));
      if (!value.empty())
         h.update(value.data(), value.size());
      return h.finalize();
   }

   void Merkleizer::clear_cache() noexcept
   {
      _memo.clear();
   }

   Checksum256 Merkleizer::hash_logical_entry(std::span<const std::byte> k,
                                              ValueTag                   t,
                                              std::span<const std::byte> v)
   {
      if (t == ValueTag::Subtree && v.size() == sizeof(std::uint32_t))
      {
         std::uint32_t sub_raw = 0;
         std::memcpy(&sub_raw, v.data(), sizeof(sub_raw));
         ArtRoot     sub_root{&_art.allocator(), PtrAddress{sub_raw}, /*adopt_existing_ref=*/false};
         Checksum256 sub_digest = compute_root(sub_root);
         return hash_entry(
             k, ValueTag::Subtree,
             {reinterpret_cast<const std::byte*>(sub_digest.data()), sub_digest.size()});
      }
      return hash_entry(k, t, v);
   }

   Checksum256 Merkleizer::compute_root(const ArtRoot& root)
   {
      if (!root)
         return Checksum256{};

      auto logical_entry_hash = [&](std::span<const std::byte> k, ValueTag t,
                                    std::span<const std::byte> v) -> Checksum256
      { return hash_logical_entry(k, t, v); };

      auto summarize_slice = [&](auto&                    self_slice,
                                 std::span<const KvEntry> slice,
                                 std::size_t              depth) -> CachedSummary
      {
         if (slice.size() == 1)
         {
            ++_stats.nodes_hashed;
            const auto& e = slice[0];
            return CachedSummary{
                .count = 1,
                .edge_prefix =
                    {e.key.begin() + static_cast<std::ptrdiff_t>(depth), e.key.end()},
                .core_hash = logical_entry_hash(e.key, e.tag, e.value),
            };
         }

         const auto& first_k = slice.front().key;
         const auto& last_k  = slice.back().key;
         std::size_t max_lcp = std::min(first_k.size(), last_k.size());
         std::size_t lcp     = 0;
         while (depth + lcp < max_lcp && first_k[depth + lcp] == last_k[depth + lcp])
            ++lcp;

         const std::size_t split_pos    = depth + lcp;
         bool              has_terminal = false;
         Checksum256       term_hash{};
         std::size_t       idx = 0;

         if (slice[0].key.size() == split_pos)
         {
            has_terminal = true;
            ++_stats.nodes_hashed;
            term_hash = logical_entry_hash(slice[0].key, slice[0].tag, slice[0].value);
            idx       = 1;
         }

         std::vector<MerkleSibling> children;
         while (idx < slice.size())
         {
            const auto        b     = std::to_integer<std::uint8_t>(slice[idx].key[split_pos]);
            const std::size_t start = idx;
            while (idx < slice.size() &&
                   std::to_integer<std::uint8_t>(slice[idx].key[split_pos]) == b)
            {
               ++idx;
            }
            CachedSummary csum = self_slice(self_slice, slice.subspan(start, idx - start),
                                            split_pos + 1);
            children.push_back(MerkleSibling{
                .branch_byte = b,
                .is_leaf     = (csum.count == 1),
                .edge_prefix = std::move(csum.edge_prefix),
                .core_hash   = csum.core_hash,
            });
         }

         ++_stats.nodes_hashed;
         return CachedSummary{
             .count = slice.size(),
             .edge_prefix =
                 {first_k.begin() + static_cast<std::ptrdiff_t>(depth),
                  first_k.begin() + static_cast<std::ptrdiff_t>(split_pos)},
             .core_hash = hash_split_core(has_terminal, term_hash, children),
         };
      };

      auto summarize_node = [&](auto&                   self_node,
                                PtrAddress              addr,
                                std::vector<std::byte>& path_buf) -> CachedSummary
      {
         const std::uint64_t memo_key =
             (fnv1a_bytes(path_buf) << 1) ^
             (static_cast<std::uint64_t>(addr.value) * 0x9E3779B97F4A7C15ull);
         if (auto it = _memo.find(memo_key); it != _memo.end())
         {
            ++_stats.cache_hits;
            return it->second;
         }

         ObjectKind             kind{};
         std::vector<std::byte> raw;
         _art.allocator().read(addr,
                               [&](ObjectKind k, std::span<const std::byte> p)
                               {
                                  kind = k;
                                  raw.assign(p.begin(), p.end());
                               });

         CachedSummary result;
         if (kind == ObjectKind::BTreeLeaf)
         {
            auto entries = decode_leaf_payload(raw);
            result       = summarize_slice(summarize_slice, entries, path_buf.size());
         }
         else
         {
            InnerView         in         = decode_inner_payload(raw);
            const std::size_t saved_size = path_buf.size();
            path_buf.insert(path_buf.end(), in.prefix.begin(), in.prefix.end());

            if (in.has_terminal && in.children.empty())
            {
               ++_stats.nodes_hashed;
               result = CachedSummary{
                   .count       = 1,
                   .edge_prefix = std::move(in.prefix),
                   .core_hash   = logical_entry_hash(path_buf, in.terminal_tag, in.terminal_val),
               };
            }
            else if (!in.has_terminal && in.children.size() == 1)
            {
               const auto [b, caddr] = in.children[0];
               path_buf.push_back(static_cast<std::byte>(b));
               CachedSummary csum = self_node(self_node, caddr, path_buf);
               path_buf.pop_back();

               std::vector<std::byte> combined = std::move(in.prefix);
               combined.push_back(static_cast<std::byte>(b));
               combined.insert(combined.end(), csum.edge_prefix.begin(), csum.edge_prefix.end());
               result = CachedSummary{
                   .count       = csum.count,
                   .edge_prefix = std::move(combined),
                   .core_hash   = csum.core_hash,
               };
            }
            else
            {
               Checksum256 term_hash{};
               if (in.has_terminal)
               {
                  ++_stats.nodes_hashed;
                  term_hash = logical_entry_hash(path_buf, in.terminal_tag, in.terminal_val);
               }

               std::vector<MerkleSibling> sibs;
               sibs.reserve(in.children.size());
               std::uint64_t total_cnt = in.has_terminal ? 1 : 0;

               for (const auto& [b, caddr] : in.children)
               {
                  path_buf.push_back(static_cast<std::byte>(b));
                  CachedSummary csum = self_node(self_node, caddr, path_buf);
                  path_buf.pop_back();
                  total_cnt += csum.count;
                  sibs.push_back(MerkleSibling{
                      .branch_byte = b,
                      .is_leaf     = (csum.count == 1),
                      .edge_prefix = std::move(csum.edge_prefix),
                      .core_hash   = csum.core_hash,
                  });
               }

               ++_stats.nodes_hashed;
               result = CachedSummary{
                   .count       = total_cnt,
                   .edge_prefix = std::move(in.prefix),
                   .core_hash   = hash_split_core(in.has_terminal, term_hash, sibs),
               };
            }
            path_buf.resize(saved_size);
         }

         _memo.emplace(memo_key, result);
         return result;
      };

      std::vector<std::byte> path_buf;
      CachedSummary          top = summarize_node(summarize_node, root.address(), path_buf);
      return hash_root_envelope(top.edge_prefix, top.core_hash);
   }

   namespace
   {
      using LogicalHashFn = std::function<Checksum256(
          std::span<const std::byte>, ValueTag, std::span<const std::byte>)>;

      // Expand ONLY the single root-to-query canonical path while summarizing off-path siblings
      CanonicalNode expand_canonical_slice(std::span<const KvEntry> slice,
                                           std::size_t              depth,
                                           const LogicalHashFn&     hash_fn)
      {
         CanonicalNode out;
         out.count = slice.size();
         if (slice.size() == 1)
         {
            const auto& e       = slice[0];
            out.edge_prefix     = {e.key.begin() + static_cast<std::ptrdiff_t>(depth), e.key.end()};
            out.core_hash       = hash_fn(e.key, e.tag, e.value);
            out.single_full_key = e.key;
            return out;
         }

         const auto& first_k = slice.front().key;
         const auto& last_k  = slice.back().key;
         std::size_t max_lcp = std::min(first_k.size(), last_k.size());
         std::size_t lcp     = 0;
         while (depth + lcp < max_lcp && first_k[depth + lcp] == last_k[depth + lcp])
            ++lcp;

         const std::size_t split_pos = depth + lcp;
         out.edge_prefix.assign(first_k.begin() + static_cast<std::ptrdiff_t>(depth),
                                first_k.begin() + static_cast<std::ptrdiff_t>(split_pos));

         std::size_t idx = 0;
         if (slice[0].key.size() == split_pos)
         {
            out.has_terminal  = true;
            out.terminal_hash = hash_fn(slice[0].key, slice[0].tag, slice[0].value);
            idx               = 1;
         }

         while (idx < slice.size())
         {
            const auto        b     = std::to_integer<std::uint8_t>(slice[idx].key[split_pos]);
            const std::size_t start = idx;
            while (idx < slice.size() &&
                   std::to_integer<std::uint8_t>(slice[idx].key[split_pos]) == b)
            {
               ++idx;
            }
            CanonicalNode cnode = expand_canonical_slice(slice.subspan(start, idx - start),
                                                         split_pos + 1, hash_fn);
            out.children.push_back(MerkleSibling{
                .branch_byte = b,
                .is_leaf     = (cnode.count == 1),
                .edge_prefix = std::move(cnode.edge_prefix),
                .core_hash   = cnode.core_hash,
            });
         }
         out.core_hash = hash_split_core(out.has_terminal, out.terminal_hash, out.children);
         return out;
      }

      CanonicalNode summarize_offpath_node(SegmentAllocator&       sal,
                                           PtrAddress              addr,
                                           std::vector<std::byte>& path_buf,
                                           const LogicalHashFn&    hash_fn)
      {
         ObjectKind             kind{};
         std::vector<std::byte> raw;
         sal.read(addr,
                  [&](ObjectKind k, std::span<const std::byte> p)
                  {
                     kind = k;
                     raw.assign(p.begin(), p.end());
                  });

         if (kind == ObjectKind::BTreeLeaf)
         {
            auto entries = decode_leaf_payload(raw);
            return expand_canonical_slice(entries, path_buf.size(), hash_fn);
         }

         InnerView         in         = decode_inner_payload(raw);
         const std::size_t saved_size = path_buf.size();
         path_buf.insert(path_buf.end(), in.prefix.begin(), in.prefix.end());

         CanonicalNode out;
         if (in.has_terminal && in.children.empty())
         {
            out.count           = 1;
            out.edge_prefix     = std::move(in.prefix);
            out.core_hash       = hash_fn(path_buf, in.terminal_tag, in.terminal_val);
            out.single_full_key = path_buf;
         }
         else if (!in.has_terminal && in.children.size() == 1)
         {
            const auto [b, caddr] = in.children[0];
            path_buf.push_back(static_cast<std::byte>(b));
            CanonicalNode cnode = summarize_offpath_node(sal, caddr, path_buf, hash_fn);
            path_buf.pop_back();

            out.count       = cnode.count;
            out.edge_prefix = std::move(in.prefix);
            out.edge_prefix.push_back(static_cast<std::byte>(b));
            out.edge_prefix.insert(out.edge_prefix.end(), cnode.edge_prefix.begin(),
                                   cnode.edge_prefix.end());
            out.core_hash       = cnode.core_hash;
            out.single_full_key = std::move(cnode.single_full_key);
            out.has_terminal    = cnode.has_terminal;
            out.terminal_hash   = cnode.terminal_hash;
            out.children        = std::move(cnode.children);
         }
         else
         {
            out.edge_prefix  = std::move(in.prefix);
            out.has_terminal = in.has_terminal;
            if (in.has_terminal)
               out.terminal_hash = hash_fn(path_buf, in.terminal_tag, in.terminal_val);

            out.count = in.has_terminal ? 1 : 0;
            out.children.reserve(in.children.size());
            for (const auto& [b, caddr] : in.children)
            {
               path_buf.push_back(static_cast<std::byte>(b));
               CanonicalNode cnode = summarize_offpath_node(sal, caddr, path_buf, hash_fn);
               path_buf.pop_back();
               out.count += cnode.count;
               out.children.push_back(MerkleSibling{
                   .branch_byte = b,
                   .is_leaf     = (cnode.count == 1),
                   .edge_prefix = std::move(cnode.edge_prefix),
                   .core_hash   = cnode.core_hash,
               });
            }
            out.core_hash = hash_split_core(out.has_terminal, out.terminal_hash, out.children);
         }

         path_buf.resize(saved_size);
         return out;
      }

      // Build proof steps along a slice of BTreeLeaf entries for `key`
      bool build_proof_in_slice(std::span<const KvEntry>      slice,
                                std::size_t                   depth,
                                std::span<const std::byte>    key,
                                bool                          want_inclusion,
                                std::vector<MerkleProofStep>& steps,
                                const LogicalHashFn&          hash_fn)
      {
         CanonicalNode node      = expand_canonical_slice(slice, depth, hash_fn);
         const auto    split_pos = depth + node.edge_prefix.size();

         const bool pfx_matches =
             key.size() >= split_pos &&
             starts_with(key.subspan(depth), node.edge_prefix);

         if (want_inclusion)
         {
            if (node.count == 1 || !pfx_matches)
               return false;

            if (key.size() == split_pos)
            {
               if (!node.has_terminal)
                  return false;
               steps.push_back(MerkleProofStep{
                   .edge_prefix   = std::move(node.edge_prefix),
                   .has_terminal  = true,
                   .terminal_hash = {},
                   .siblings      = std::move(node.children),
               });
               return true;
            }

            const auto b = std::to_integer<std::uint8_t>(key[split_pos]);
            std::vector<MerkleSibling> sibs_without_b;
            bool                       found_b   = false;
            bool                       b_is_leaf = false;
            for (const auto& c : node.children)
            {
               if (c.branch_byte == b)
               {
                  found_b   = true;
                  b_is_leaf = c.is_leaf;
               }
               else
               {
                  sibs_without_b.push_back(c);
               }
            }
            if (!found_b)
               return false;

            steps.push_back(MerkleProofStep{
                .edge_prefix   = std::move(node.edge_prefix),
                .has_terminal  = node.has_terminal,
                .terminal_hash = node.terminal_hash,
                .siblings      = std::move(sibs_without_b),
            });

            if (b_is_leaf)
            {
               // Verify exact key exists in slice
               auto it = std::lower_bound(slice.begin(), slice.end(), key,
                                          [](const KvEntry& e, std::span<const std::byte> k)
                                          { return lex_compare(e.key, k) < 0; });
               return it != slice.end() && lex_compare(it->key, key) == 0;
            }

            // Recurse into matching bucket
            std::size_t idx = (slice[0].key.size() == split_pos) ? 1 : 0;
            while (idx < slice.size() &&
                   std::to_integer<std::uint8_t>(slice[idx].key[split_pos]) != b)
            {
               ++idx;
            }
            const std::size_t start = idx;
            while (idx < slice.size() &&
                   std::to_integer<std::uint8_t>(slice[idx].key[split_pos]) == b)
            {
               ++idx;
            }
            return build_proof_in_slice(slice.subspan(start, idx - start), split_pos + 1, key,
                                        true, steps, hash_fn);
         }

         // Non-inclusion in slice (node.count >= 2)
         if (!pfx_matches || key.size() == split_pos)
         {
            if (pfx_matches && key.size() == split_pos && node.has_terminal)
               return false;  // Key actually exists!
            steps.push_back(MerkleProofStep{
                .edge_prefix   = std::move(node.edge_prefix),
                .has_terminal  = node.has_terminal,
                .terminal_hash = node.terminal_hash,
                .siblings      = std::move(node.children),
            });
            return true;
         }

         const auto b   = std::to_integer<std::uint8_t>(key[split_pos]);
         auto       cit = std::find_if(node.children.begin(), node.children.end(),
                                       [b](const MerkleSibling& s) { return s.branch_byte == b; });
         if (cit == node.children.end())
         {
            steps.push_back(MerkleProofStep{
                .edge_prefix   = std::move(node.edge_prefix),
                .has_terminal  = node.has_terminal,
                .terminal_hash = node.terminal_hash,
                .siblings      = std::move(node.children),
            });
            return true;
         }

         const auto rem = key.subspan(split_pos + 1);
         if (cit->is_leaf)
         {
            if (lex_compare(rem, cit->edge_prefix) == 0)
               return false;  // Key actually exists!
            steps.push_back(MerkleProofStep{
                .edge_prefix   = std::move(node.edge_prefix),
                .has_terminal  = node.has_terminal,
                .terminal_hash = node.terminal_hash,
                .siblings      = std::move(node.children),
            });
            return true;
         }

         // Child is a split node: if `rem` doesn't even start with `cit->edge_prefix`, descend
         // one more step so Stop A triggers cleanly on that split node.
         std::vector<MerkleSibling> sibs_without_b;
         for (const auto& c : node.children)
         {
            if (c.branch_byte != b)
               sibs_without_b.push_back(c);
         }
         steps.push_back(MerkleProofStep{
             .edge_prefix   = std::move(node.edge_prefix),
             .has_terminal  = node.has_terminal,
             .terminal_hash = node.terminal_hash,
             .siblings      = std::move(sibs_without_b),
         });

         std::size_t idx = (slice[0].key.size() == split_pos) ? 1 : 0;
         while (idx < slice.size() &&
                std::to_integer<std::uint8_t>(slice[idx].key[split_pos]) != b)
         {
            ++idx;
         }
         const std::size_t start = idx;
         while (idx < slice.size() &&
                std::to_integer<std::uint8_t>(slice[idx].key[split_pos]) == b)
         {
            ++idx;
         }
         return build_proof_in_slice(slice.subspan(start, idx - start), split_pos + 1, key, false,
                                     steps, hash_fn);
      }

      bool build_proof_in_node(SegmentAllocator&             sal,
                               PtrAddress                    addr,
                               std::vector<std::byte>&       path_buf,
                               std::vector<std::byte>&       carried_prefix,
                               std::span<const std::byte>    key,
                               bool                          want_inclusion,
                               std::vector<MerkleProofStep>& steps,
                               const LogicalHashFn&          hash_fn)
      {
         ObjectKind             kind{};
         std::vector<std::byte> raw;
         sal.read(addr,
                  [&](ObjectKind k, std::span<const std::byte> p)
                  {
                     kind = k;
                     raw.assign(p.begin(), p.end());
                  });

         if (kind == ObjectKind::BTreeLeaf)
         {
            auto              entries = decode_leaf_payload(raw);
            const std::size_t start_d = path_buf.size() - carried_prefix.size();
            return build_proof_in_slice(entries, start_d, key, want_inclusion, steps, hash_fn);
         }

         InnerView         in         = decode_inner_payload(raw);
         const std::size_t saved_path = path_buf.size();
         const std::size_t saved_car  = carried_prefix.size();
         path_buf.insert(path_buf.end(), in.prefix.begin(), in.prefix.end());
         carried_prefix.insert(carried_prefix.end(), in.prefix.begin(), in.prefix.end());

         auto restore = [&](bool ok)
         {
            path_buf.resize(saved_path);
            carried_prefix.resize(saved_car);
            return ok;
         };

         if (!in.has_terminal && in.children.size() == 1)
         {
            const auto [b, caddr] = in.children[0];
            path_buf.push_back(static_cast<std::byte>(b));
            carried_prefix.push_back(static_cast<std::byte>(b));
            bool ok = build_proof_in_node(sal, caddr, path_buf, carried_prefix, key, want_inclusion,
                                          steps, hash_fn);
            return restore(ok);
         }

         // Canonical split node! `effective_prefix` since parent split is `carried_prefix`.
         const std::size_t start_d   = path_buf.size() - carried_prefix.size();
         const std::size_t split_pos = path_buf.size();
         const bool        pfx_matches =
             key.size() >= split_pos &&
             starts_with(key.subspan(start_d), carried_prefix);

         Checksum256 term_hash{};
         if (in.has_terminal)
            term_hash = hash_fn(path_buf, in.terminal_tag, in.terminal_val);

         if (want_inclusion)
         {
            if (!pfx_matches)
               return restore(false);

            if (key.size() == split_pos)
            {
               if (!in.has_terminal)
                  return restore(false);
               std::vector<MerkleSibling> all_sibs;
               all_sibs.reserve(in.children.size());
               for (const auto& [b, caddr] : in.children)
               {
                  path_buf.push_back(static_cast<std::byte>(b));
                  CanonicalNode cn = summarize_offpath_node(sal, caddr, path_buf, hash_fn);
                  path_buf.pop_back();
                  all_sibs.push_back(MerkleSibling{
                      .branch_byte = b,
                      .is_leaf     = (cn.count == 1),
                      .edge_prefix = std::move(cn.edge_prefix),
                      .core_hash   = cn.core_hash,
                  });
               }
               steps.push_back(MerkleProofStep{
                   .edge_prefix   = carried_prefix,
                   .has_terminal  = true,
                   .terminal_hash = {},
                   .siblings      = std::move(all_sibs),
               });
               return restore(true);
            }

            const auto target_b = std::to_integer<std::uint8_t>(key[split_pos]);
            PtrAddress target_addr{};
            std::vector<MerkleSibling> sibs_without_b;
            for (const auto& [b, caddr] : in.children)
            {
               if (b == target_b)
               {
                  target_addr = caddr;
               }
               else
               {
                  path_buf.push_back(static_cast<std::byte>(b));
                  CanonicalNode cn = summarize_offpath_node(sal, caddr, path_buf, hash_fn);
                  path_buf.pop_back();
                  sibs_without_b.push_back(MerkleSibling{
                      .branch_byte = b,
                      .is_leaf     = (cn.count == 1),
                      .edge_prefix = std::move(cn.edge_prefix),
                      .core_hash   = cn.core_hash,
                  });
               }
            }
            if (!target_addr)
               return restore(false);

            steps.push_back(MerkleProofStep{
                .edge_prefix   = carried_prefix,
                .has_terminal  = in.has_terminal,
                .terminal_hash = term_hash,
                .siblings      = std::move(sibs_without_b),
            });

            path_buf.push_back(static_cast<std::byte>(target_b));
            CanonicalNode target_cn = summarize_offpath_node(sal, target_addr, path_buf, hash_fn);
            if (target_cn.count == 1)
            {
               path_buf.pop_back();
               return restore(lex_compare(target_cn.single_full_key, key) == 0);
            }
            std::vector<std::byte> fresh_carry;
            bool ok = build_proof_in_node(sal, target_addr, path_buf, fresh_carry, key, true,
                                          steps, hash_fn);
            path_buf.pop_back();
            return restore(ok);
         }

         // Non-inclusion at canonical split `in`
         auto summarize_all_children = [&]()
         {
            std::vector<MerkleSibling> all_sibs;
            all_sibs.reserve(in.children.size());
            for (const auto& [b, caddr] : in.children)
            {
               path_buf.push_back(static_cast<std::byte>(b));
               CanonicalNode cn = summarize_offpath_node(sal, caddr, path_buf, hash_fn);
               path_buf.pop_back();
               all_sibs.push_back(MerkleSibling{
                   .branch_byte = b,
                   .is_leaf     = (cn.count == 1),
                   .edge_prefix = std::move(cn.edge_prefix),
                   .core_hash   = cn.core_hash,
               });
            }
            return all_sibs;
         };

         if (!pfx_matches || key.size() == split_pos)
         {
            if (pfx_matches && key.size() == split_pos && in.has_terminal)
               return restore(false);
            steps.push_back(MerkleProofStep{
                .edge_prefix   = carried_prefix,
                .has_terminal  = in.has_terminal,
                .terminal_hash = term_hash,
                .siblings      = summarize_all_children(),
            });
            return restore(true);
         }

         const auto target_b = std::to_integer<std::uint8_t>(key[split_pos]);
         auto       cit      = std::lower_bound(in.children.begin(), in.children.end(), target_b,
                                                [](const auto& p, std::uint8_t b) { return p.first < b; });
         if (cit == in.children.end() || cit->first != target_b)
         {
            steps.push_back(MerkleProofStep{
                .edge_prefix   = carried_prefix,
                .has_terminal  = in.has_terminal,
                .terminal_hash = term_hash,
                .siblings      = summarize_all_children(),
            });
            return restore(true);
         }

         path_buf.push_back(static_cast<std::byte>(target_b));
         CanonicalNode target_cn = summarize_offpath_node(sal, cit->second, path_buf, hash_fn);
         path_buf.pop_back();

         if (target_cn.count == 1)
         {
            if (lex_compare(target_cn.single_full_key, key) == 0)
               return restore(false);
            steps.push_back(MerkleProofStep{
                .edge_prefix   = carried_prefix,
                .has_terminal  = in.has_terminal,
                .terminal_hash = term_hash,
                .siblings      = summarize_all_children(),
            });
            return restore(true);
         }

         // Target child is itself a split node: emit siblings excluding target_b and recurse
         std::vector<MerkleSibling> sibs_without_b;
         for (const auto& [b, caddr] : in.children)
         {
            if (b == target_b)
               continue;
            path_buf.push_back(static_cast<std::byte>(b));
            CanonicalNode cn = summarize_offpath_node(sal, caddr, path_buf, hash_fn);
            path_buf.pop_back();
            sibs_without_b.push_back(MerkleSibling{
                .branch_byte = b,
                .is_leaf     = (cn.count == 1),
                .edge_prefix = std::move(cn.edge_prefix),
                .core_hash   = cn.core_hash,
            });
         }
         steps.push_back(MerkleProofStep{
             .edge_prefix   = carried_prefix,
             .has_terminal  = in.has_terminal,
             .terminal_hash = term_hash,
             .siblings      = std::move(sibs_without_b),
         });

         path_buf.push_back(static_cast<std::byte>(target_b));
         std::vector<std::byte> fresh_carry;
         bool ok = build_proof_in_node(sal, cit->second, path_buf, fresh_carry, key, false, steps,
                                       hash_fn);
         path_buf.pop_back();
         return restore(ok);
      }

      bool strictly_sorted_siblings(std::span<const MerkleSibling> sibs) noexcept
      {
         for (std::size_t i = 1; i < sibs.size(); ++i)
         {
            if (sibs[i - 1].branch_byte >= sibs[i].branch_byte)
               return false;
         }
         return true;
      }

      std::vector<MerkleSibling> insert_sibling_sorted(std::span<const MerkleSibling> sibs,
                                                       MerkleSibling                  extra,
                                                       bool&                          duplicate)
      {
         std::vector<MerkleSibling> out;
         out.reserve(sibs.size() + 1);
         bool inserted = false;
         duplicate     = false;
         for (const auto& s : sibs)
         {
            if (s.branch_byte == extra.branch_byte)
            {
               duplicate = true;
               return {};
            }
            if (!inserted && extra.branch_byte < s.branch_byte)
            {
               out.push_back(std::move(extra));
               inserted = true;
            }
            out.push_back(s);
         }
         if (!inserted)
            out.push_back(std::move(extra));
         return out;
      }
   }  // namespace

   MerkleProof Merkleizer::prove_inclusion(const ArtRoot& root, std::span<const std::byte> key)
   {
      MerkleProof proof;
      proof.target_key.assign(key.begin(), key.end());
      if (!root)
         return proof;

      LogicalHashFn hash_fn = [this](std::span<const std::byte> k, ValueTag t,
                                     std::span<const std::byte> v)
      { return hash_logical_entry(k, t, v); };

      if (_art.count(root) == 1)
      {
         std::vector<std::byte> path_buf;
         CanonicalNode cn =
             summarize_offpath_node(_art.allocator(), root.address(), path_buf, hash_fn);
         if (lex_compare(cn.single_full_key, key) != 0)
            return proof;
         proof.is_inclusion = true;
         proof.single_entry = true;
         proof.root_prefix.assign(key.begin(), key.end());
         return proof;
      }

      std::vector<std::byte> path_buf;
      std::vector<std::byte> carried_prefix;
      if (!build_proof_in_node(_art.allocator(), root.address(), path_buf, carried_prefix, key,
                               true, proof.steps, hash_fn))
      {
         proof.steps.clear();
         return proof;
      }
      proof.is_inclusion = true;
      return proof;
   }

   MerkleProof Merkleizer::prove_non_inclusion(const ArtRoot& root, std::span<const std::byte> key)
   {
      MerkleProof proof;
      proof.is_inclusion = false;
      proof.target_key.assign(key.begin(), key.end());
      if (!root)
      {
         proof.empty_tree = true;
         return proof;
      }

      LogicalHashFn hash_fn = [this](std::span<const std::byte> k, ValueTag t,
                                     std::span<const std::byte> v)
      { return hash_logical_entry(k, t, v); };

      if (_art.count(root) == 1)
      {
         std::vector<std::byte> path_buf;
         CanonicalNode cn =
             summarize_offpath_node(_art.allocator(), root.address(), path_buf, hash_fn);
         if (lex_compare(cn.single_full_key, key) == 0)
            return MerkleProof{};  // Key exists -> cannot prove non-inclusion
         proof.single_entry      = true;
         proof.witness_key       = std::move(cn.single_full_key);
         proof.witness_core_hash = cn.core_hash;
         return proof;
      }

      std::vector<std::byte> path_buf;
      std::vector<std::byte> carried_prefix;
      if (!build_proof_in_node(_art.allocator(), root.address(), path_buf, carried_prefix, key,
                               false, proof.steps, hash_fn))
      {
         proof.steps.clear();
      }
      return proof;
   }

   bool Merkleizer::verify_inclusion(const Checksum256&         expected_root,
                                     std::span<const std::byte> key,
                                     std::span<const std::byte> value,
                                     const MerkleProof&         proof,
                                     ValueTag                   tag)
   {
      if (!proof.is_inclusion || proof.empty_tree || expected_root == Checksum256{} ||
          lex_compare(proof.target_key, key) != 0)
         return false;

      const Checksum256 entry_hash = hash_entry(key, tag, value);

      if (proof.single_entry)
      {
         if (!proof.steps.empty() || lex_compare(proof.root_prefix, key) != 0)
            return false;
         return hash_root_envelope(key, entry_hash) == expected_root;
      }

      if (proof.steps.empty())
         return false;

      const std::size_t        k = proof.steps.size();
      std::vector<std::size_t> split_positions(k, 0);
      std::size_t              depth = 0;

      for (std::size_t i = 0; i < k; ++i)
      {
         const auto& step = proof.steps[i];
         if (!strictly_sorted_siblings(step.siblings))
            return false;
         if (key.size() < depth + step.edge_prefix.size() ||
             !starts_with(key.subspan(depth), step.edge_prefix))
         {
            return false;
         }
         split_positions[i] = depth + step.edge_prefix.size();
         if (i + 1 < k)
         {
            if (key.size() <= split_positions[i])
               return false;
            depth = split_positions[i] + 1;
         }
      }

      Checksum256 cur_core{};
      {
         const auto&       last      = proof.steps[k - 1];
         const std::size_t split_pos = split_positions[k - 1];
         if (key.size() == split_pos)
         {
            if (!last.has_terminal || last.siblings.empty())
               return false;
            cur_core = hash_split_core(true, entry_hash, last.siblings);
         }
         else
         {
            if (!last.has_terminal && last.siblings.empty())
               return false;
            const auto    b   = std::to_integer<std::uint8_t>(key[split_pos]);
            const auto    rem = key.subspan(split_pos + 1);
            bool          dup = false;
            MerkleSibling leaf_sib{
                .branch_byte = b,
                .is_leaf     = true,
                .edge_prefix = {rem.begin(), rem.end()},
                .core_hash   = entry_hash,
            };
            auto merged = insert_sibling_sorted(last.siblings, std::move(leaf_sib), dup);
            if (dup)
               return false;
            cur_core = hash_split_core(last.has_terminal, last.terminal_hash, merged);
         }
      }

      for (std::size_t idx = k - 1; idx > 0; --idx)
      {
         const std::size_t i         = idx - 1;
         const auto&       step      = proof.steps[i];
         const auto        b         = std::to_integer<std::uint8_t>(key[split_positions[i]]);
         bool              dup       = false;
         MerkleSibling     child_sib = {
                 .branch_byte = b,
                 .is_leaf     = false,
                 .edge_prefix = proof.steps[i + 1].edge_prefix,
                 .core_hash   = cur_core,
         };
         auto merged = insert_sibling_sorted(step.siblings, std::move(child_sib), dup);
         if (dup || (!step.has_terminal && merged.size() < 2))
            return false;
         cur_core = hash_split_core(step.has_terminal, step.terminal_hash, merged);
      }

      return hash_root_envelope(proof.steps[0].edge_prefix, cur_core) == expected_root;
   }

   bool Merkleizer::verify_non_inclusion(const Checksum256&         expected_root,
                                         std::span<const std::byte> key,
                                         const MerkleProof&         proof)
   {
      if (proof.is_inclusion || lex_compare(proof.target_key, key) != 0)
         return false;

      if (proof.empty_tree)
      {
         return proof.steps.empty() && !proof.single_entry && expected_root == Checksum256{};
      }

      if (expected_root == Checksum256{})
         return false;

      if (proof.single_entry)
      {
         if (!proof.steps.empty() || lex_compare(proof.witness_key, key) == 0)
            return false;
         return hash_root_envelope(proof.witness_key, proof.witness_core_hash) == expected_root;
      }

      if (proof.steps.empty())
         return false;

      const std::size_t        k = proof.steps.size();
      std::vector<std::size_t> split_positions(k, 0);
      std::size_t              depth = 0;

      for (std::size_t i = 0; i + 1 < k; ++i)
      {
         const auto& step = proof.steps[i];
         if (!strictly_sorted_siblings(step.siblings))
            return false;
         if (key.size() <= depth + step.edge_prefix.size() ||
             !starts_with(key.subspan(depth), step.edge_prefix))
         {
            return false;
         }
         split_positions[i] = depth + step.edge_prefix.size();
         depth              = split_positions[i] + 1;
      }

      // Check final split node proves absence via Stop A, B, C, or D
      const auto& last = proof.steps[k - 1];
      if (!strictly_sorted_siblings(last.siblings))
         return false;
      if ((last.has_terminal && last.siblings.empty()) ||
          (!last.has_terminal && last.siblings.size() < 2))
      {
         return false;
      }

      const std::size_t split_pos = depth + last.edge_prefix.size();
      const bool        pfx_matches =
          key.size() >= split_pos && starts_with(key.subspan(depth), last.edge_prefix);

      bool proven_absent = false;
      if (!pfx_matches)
      {
         proven_absent = true;  // Stop A: key diverges inside edge_prefix
      }
      else if (key.size() == split_pos)
      {
         proven_absent = !last.has_terminal;  // Stop B: no terminal at split_pos
      }
      else
      {
         const auto b   = std::to_integer<std::uint8_t>(key[split_pos]);
         auto       cit = std::find_if(last.siblings.begin(), last.siblings.end(),
                                       [b](const MerkleSibling& s) { return s.branch_byte == b; });
         if (cit == last.siblings.end())
         {
            proven_absent = true;  // Stop C: branch byte missing
         }
         else if (cit->is_leaf)
         {
            const auto rem = key.subspan(split_pos + 1);
            proven_absent  = (lex_compare(rem, cit->edge_prefix) != 0);  // Stop D: leaf key != key
         }
      }

      if (!proven_absent)
         return false;

      Checksum256 cur_core = hash_split_core(last.has_terminal, last.terminal_hash, last.siblings);

      for (std::size_t idx = k - 1; idx > 0; --idx)
      {
         const std::size_t i         = idx - 1;
         const auto&       step      = proof.steps[i];
         const auto        b         = std::to_integer<std::uint8_t>(key[split_positions[i]]);
         bool              dup       = false;
         MerkleSibling     child_sib = {
                 .branch_byte = b,
                 .is_leaf     = false,
                 .edge_prefix = proof.steps[i + 1].edge_prefix,
                 .core_hash   = cur_core,
         };
         auto merged = insert_sibling_sorted(step.siblings, std::move(child_sib), dup);
         if (dup || (!step.has_terminal && merged.size() < 2))
            return false;
         cur_core = hash_split_core(step.has_terminal, step.terminal_hash, merged);
      }

      return hash_root_envelope(proof.steps[0].edge_prefix, cur_core) == expected_root;
   }

   // =========================================================================
   // Anchored JSON Proof Serialization & Verification
   // =========================================================================

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
               char c = _s[_pos++];
               if (c == '"')
                  return out;
               if (c == '\\')
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
                  out.push_back(c);
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
                   return parse_checksum(bundle.anchor.state_root);
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
                return true;
             }
             if (k == "anchor")
                return parse_anchor();
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
                return parse_proof();
             return false;
          });

      if (!ok || !p.eof() || bundle.version != "muabase.dwal_merkle.v1")
         return std::nullopt;
      return bundle;
   }

   bool verify_anchored_proof(const AnchoredMerkleProof& bundle)
   {
      if (bundle.version != "muabase.dwal_merkle.v1")
         return false;
      if (bundle.proof.is_inclusion)
      {
         return Merkleizer::verify_inclusion(bundle.anchor.state_root, bundle.proof.target_key,
                                             bundle.value, bundle.proof, bundle.value_tag);
      }
      return Merkleizer::verify_non_inclusion(bundle.anchor.state_root, bundle.proof.target_key,
                                              bundle.proof);
   }
}  // namespace muabase_db
