#pragma once

#include <cstdlib>
#include <exception>
#include <functional>
#include <iostream>
#include <source_location>
#include <string>
#include <string_view>
#include <vector>

namespace bdd_lite
{
   struct scenario_entry
   {
      std::string           name;
      std::function<void()> fn;
   };

   inline std::vector<scenario_entry>& registry()
   {
      static std::vector<scenario_entry> r;
      return r;
   }

   inline int& assertion_count()
   {
      static int c = 0;
      return c;
   }

   struct registrar
   {
      registrar(std::string name, std::function<void()> fn)
      {
         registry().push_back({std::move(name), std::move(fn)});
      }
   };

   struct assertion_failure : std::exception
   {
      std::string msg;
      explicit assertion_failure(std::string m) : msg(std::move(m)) {}
      const char* what() const noexcept override { return msg.c_str(); }
   };

   inline void check_impl(bool                 cond,
                          std::string_view     expr,
                          std::source_location loc = std::source_location::current())
   {
      ++assertion_count();
      if (!cond)
      {
         throw assertion_failure(std::string(loc.file_name()) + ":" + std::to_string(loc.line()) +
                                 ": REQUIRE(" + std::string(expr) + ") failed");
      }
   }

   inline int run_all(std::string_view suite_name)
   {
      int passed = 0;
      int failed = 0;
      std::cout << "[BDD Suite] " << suite_name << " (" << registry().size() << " scenarios)\n";
      for (const auto& s : registry())
      {
         try
         {
            s.fn();
            ++passed;
            std::cout << "  [PASS] Scenario: " << s.name << "\n";
         }
         catch (const std::exception& e)
         {
            ++failed;
            std::cerr << "  [FAIL] Scenario: " << s.name << "\n         " << e.what() << "\n";
         }
      }
      std::cout << "[Summary] " << passed << " passed, " << failed << " failed, "
                << assertion_count() << " assertions\n";
      return failed == 0 ? 0 : 1;
   }
}  // namespace bdd_lite

#define BDD_CONCAT_INNER(a, b) a##b
#define BDD_CONCAT(a, b) BDD_CONCAT_INNER(a, b)

#define SCENARIO(name)                                                  \
   static void                BDD_CONCAT(bdd_scenario_fn_, __LINE__)(); \
   static bdd_lite::registrar BDD_CONCAT(bdd_scenario_reg_, __LINE__)(  \
       name, BDD_CONCAT(bdd_scenario_fn_, __LINE__));                   \
   static void BDD_CONCAT(bdd_scenario_fn_, __LINE__)()

#define GIVEN(desc) if ([[maybe_unused]] const char* bdd_given_ = (desc); true)
#define WHEN(desc) if ([[maybe_unused]] const char* bdd_when_ = (desc); true)
#define THEN(desc) if ([[maybe_unused]] const char* bdd_then_ = (desc); true)
#define AND_THEN(desc) if ([[maybe_unused]] const char* bdd_and_then_ = (desc); true)
#define REQUIRE(expr) ::bdd_lite::check_impl(static_cast<bool>(expr), #expr)
