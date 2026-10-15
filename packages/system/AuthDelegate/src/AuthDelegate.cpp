#include <services/system/AuthDelegate.hpp>

#include <psibase/dispatch.hpp>
#include <services/system/Accounts.hpp>
#include <services/system/AuthAny.hpp>
#include <services/system/StagedTx.hpp>
#include "services/system/Transact.hpp"

using namespace psibase;

namespace SystemService
{
   bool AuthDelegate::checkAuthSys(std::uint32_t      flags,
                                   AccountNumber      sender,
                                   ServiceMethod      action,
                                   std::vector<Claim> claims)
   {
      auto endTable = open<EndowmentPolicyTable>();
      if (auto endRecord = endTable.getIndex<0>().get(sender))
      {
         if (endRecord->is_endowment)
         {
            // Endowment guard active: reject outward consumption.
            // Allowed action services: the approved investment targets, the distributable yield account, or the auth service itself.
            bool allowed = (action.service == endRecord->distributable_yield_account);
            if (!allowed)
            {
               for (const auto& target : endRecord->approved_investment_targets)
               {
                  if (action.service == target)
                  {
                     allowed = true;
                     break;
                  }
               }
            }
            if (!allowed && action.service != AuthDelegate::service)
            {
               abortMessage("EndowmentPolicy: principal transfer to unapproved target rejected");
            }
         }
      }

      auto owner = checkOwner(sender);

      auto _ = recurse();
      return authServiceOf(owner).checkAuthSys(flags, owner, std::move(action), std::move(claims));
   }

   void AuthDelegate::canAuthUserSys(psibase::AccountNumber user)
   {
      checkOwner(user);
   }

   std::vector<AccountNumber> AuthDelegate::getDlgsSys(psibase::AccountNumber sender)
   {
      return {checkOwner(sender)};
   }

   bool AuthDelegate::isAuthSys(psibase::AccountNumber              sender,
                                std::vector<psibase::AccountNumber> authorizers)
   {
      return std::ranges::contains(authorizers, checkOwner(sender));
   }

   bool AuthDelegate::isRejectSys(psibase::AccountNumber              sender,
                                  std::vector<psibase::AccountNumber> rejecters)
   {
      return std::ranges::contains(rejecters, checkOwner(sender));
   }

   void AuthDelegate::setOwner(psibase::AccountNumber owner)
   {
      auto table  = open<AuthDelegateTable>();
      auto record = table.getIndex<0>().get(owner);
      auto sender = getSender();
      if (sender == owner || record && record->owner == sender)
      {
         abortMessage("circular ownership");
      }

      to<Accounts>().incAuthSeq(sender);

      table.put(AuthDelegateRecord{.account = getSender(), .owner = owner});
   }

   std::optional<psibase::AccountNumber> AuthDelegate::getOwner(psibase::AccountNumber account)
   {
      if (auto row = open<AuthDelegateTable>(KvMode::read).getIndex<0>().get(account))
         return row->owner;
      return std::nullopt;
   }

   psibase::AccountNumber AuthDelegate::checkOwner(psibase::AccountNumber account)
   {
      auto owner = getOwner(account);
      check(owner.has_value(), "account does not have an owning account");
      return *owner;
   }

   Actor<AuthInterface> AuthDelegate::authServiceOf(psibase::AccountNumber account)
   {
      return Actor<AuthInterface>{service, to<Accounts>().getAuthOf(account)};
   }

   // cases to consider:
   // - the account does not exist
   // - the account exists and uses AuthDelegate with another owner
   // - the account exists and uses a different auth service
   // - the account exists and uses a different auth service but there is a record in auth delegate
   bool AuthDelegate::newAccount(psibase::AccountNumber name,
                                 psibase::AccountNumber owner,
                                 NewAccountMode         mode)
   {
      auto table = open<AuthDelegateTable>();
      if (mode == NewAccountMode::matchExisting)
      {
         auto record = table.getIndex<0>().get(name);
         if (record && record->owner != owner)
         {
            abortMessage(std::format("{} is owned by {}", name.str(), record->owner.str()));
         }
      }

      check(to<Accounts>().exists(owner), "owner account does not exist");

      bool created = to<Accounts>().newAccount(name, AuthDelegate::service, mode);
      if (created)
         table.put(AuthDelegateRecord{.account = name, .owner = owner});
      return created;
   }

   void AuthDelegate::setEndowmentPolicy(AccountNumber              account,
                                         AccountNumber              yield_account,
                                         std::vector<AccountNumber> approved_investments)
   {
      auto sender = getSender();
      check(sender == account || getOwner(account) == sender, "Unauthorized to set endowment policy");
      auto endTable = open<EndowmentPolicyTable>();
      endTable.put(EndowmentPolicyRecord{
         .account = account,
         .distributable_yield_account = yield_account,
         .approved_investment_targets = approved_investments,
         .is_endowment = true
      });
   }

}  // namespace SystemService

PSIBASE_DISPATCH(SystemService::AuthDelegate)
