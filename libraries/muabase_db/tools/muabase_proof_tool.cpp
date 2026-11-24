#include <muabase_db/dwal_merkle.hpp>

#include <cstdio>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <string_view>
#include <unistd.h>

using namespace muabase_db;

namespace
{
   std::span<const std::byte> bspan(std::string_view sv)
   {
      return {reinterpret_cast<const std::byte*>(sv.data()), sv.size()};
   }

   int emit_vectors()
   {
      auto dir = std::filesystem::temp_directory_path() /
                 ("muabase_proof_tool_" + std::to_string(::getpid()));
      std::filesystem::remove_all(dir);
      std::filesystem::create_directories(dir);

      SegmentAllocator sal{dir, SalConfig{.segment_bytes = 64 * 1024}};
      CowArt           art{sal};
      Merkleizer       merkle{art};

      ArtRoot child;
      child = art.upsert(child, bspan("zakat/1"), bspan("100000"));
      child = art.upsert(child, bspan("zakat/2"), bspan("250000"));
      Checksum256 child_root = merkle.compute_root(child);

      ArtRoot root;
      root = art.upsert(root, bspan("receipt/0001"), bspan("{\"amount\":100000,\"fee\":0}"));
      root = art.upsert(root, bspan("receipt/0001/detail"), bspan("{\"corridor\":\"TR\"}"));
      root = art.upsert(root, bspan("receipt/0002"), bspan("{\"amount\":250000,\"fee\":0}"));
      root = art.upsert_subtree(root, bspan("subtree/zakat"), child);
      Checksum256 state_root = merkle.compute_root(root);

      auto make_anchor = [&]()
      {
         return BlockHeaderAnchor{
             .block_num        = 100,
             .block_id         = "00000064" + checksum_to_hex(state_root).substr(8),
             .producer         = "qirim-ist-1",
             .state_root       = state_root,
             .header_signature = "SIG_K1_vector_100",
         };
      };

      // 1) Leaf inclusion
      AnchoredMerkleProof leaf_inc;
      leaf_inc.anchor = make_anchor();
      leaf_inc.value  = art.get(root, bspan("receipt/0002")).value();
      leaf_inc.proof  = merkle.prove_inclusion(root, bspan("receipt/0002"));

      // 2) Prefix-terminal inclusion
      AnchoredMerkleProof term_inc;
      term_inc.anchor = make_anchor();
      term_inc.value  = art.get(root, bspan("receipt/0001")).value();
      term_inc.proof  = merkle.prove_inclusion(root, bspan("receipt/0001"));

      // 3) Subtree inclusion
      AnchoredMerkleProof sub_inc;
      sub_inc.anchor    = make_anchor();
      sub_inc.value_tag = ValueTag::Subtree;
      sub_inc.value.assign(reinterpret_cast<const std::byte*>(child_root.data()),
                           reinterpret_cast<const std::byte*>(child_root.data()) +
                               child_root.size());
      sub_inc.proof = merkle.prove_inclusion(root, bspan("subtree/zakat"));

      // 4) Non-inclusion gap
      AnchoredMerkleProof absent;
      absent.anchor = make_anchor();
      absent.proof  = merkle.prove_non_inclusion(root, bspan("receipt/0003"));

      // 5) Single-entry inclusion
      ArtRoot             solo;
      solo                    = art.upsert(solo, bspan("waqf/corpus"), bspan("locked:1000000"));
      Checksum256 solo_root   = merkle.compute_root(solo);
      AnchoredMerkleProof s_inc;
      s_inc.anchor = BlockHeaderAnchor{.block_num  = 101,
                                       .block_id   = "00000065" + checksum_to_hex(solo_root).substr(8),
                                       .producer   = "masjid-prod-1",
                                       .state_root = solo_root};
      s_inc.value  = art.get(solo, bspan("waqf/corpus")).value();
      s_inc.proof  = merkle.prove_inclusion(solo, bspan("waqf/corpus"));

      std::cout << "{\"leafInclusion\":" << anchored_proof_to_json(leaf_inc)
                << ",\"terminalInclusion\":" << anchored_proof_to_json(term_inc)
                << ",\"subtreeInclusion\":" << anchored_proof_to_json(sub_inc)
                << ",\"nonInclusion\":" << anchored_proof_to_json(absent)
                << ",\"singleEntryInclusion\":" << anchored_proof_to_json(s_inc) << "}\n";

      std::error_code ec;
      std::filesystem::remove_all(dir, ec);
      return 0;
   }

   int verify_file(const std::string& path)
   {
      std::ifstream in(path);
      if (!in)
      {
         std::cerr << "Unable to open proof file: " << path << "\n";
         return 2;
      }
      std::ostringstream ss;
      ss << in.rdbuf();
      auto parsed = anchored_proof_from_json(ss.str());
      if (!parsed)
      {
         std::cerr << "Invalid anchored proof JSON\n";
         return 1;
      }
      if (!verify_anchored_proof(*parsed))
      {
         std::cerr << "Proof verification FAILED\n";
         return 1;
      }
      std::cout << "VERIFIED block=" << parsed->anchor.block_num
                << " root=" << checksum_to_hex(parsed->anchor.state_root)
                << " mode=" << (parsed->proof.is_inclusion ? "inclusion" : "non-inclusion") << "\n";
      return 0;
   }
}  // namespace

int main(int argc, char** argv)
{
   if (argc == 2 && std::string_view(argv[1]) == "--emit-vectors")
      return emit_vectors();
   if (argc == 3 && std::string_view(argv[1]) == "--verify")
      return verify_file(argv[2]);

   std::cerr << "Usage:\n"
             << "  muabase-proof-tool --emit-vectors\n"
             << "  muabase-proof-tool --verify <proof.json>\n";
   return 2;
}
