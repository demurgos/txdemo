use crate::core::{cmd, Account, AccountBalance, ClientId, Command, Transaction, TransactionId};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use thiserror::Error;

/// How to handle disputes related to withdrawal transactions.
pub enum WithdrawalDisputePolicy {
    /// Ignore the dispute if it relates to a withdrawal transaction.
    Deny,
    /// Allow the dispute only if the amount is less than the available assets.
    /// (Allows to always seize the account and recover the refund in case of
    /// fraudulent chargeback)
    IfMoreAvailableThanDisputed,
}

pub struct MemAccountService {
    withdrawal_dispute_policy: WithdrawalDisputePolicy,
    /// All the processed transactions
    ///
    /// ## Invariants
    ///
    /// The id of the transaction matches the corresponding key in the hashmap.
    ///
    /// ```
    /// let tx = self.transactions.get(tx_id);
    /// if let Some(tx) = tx {
    ///     assert_eq!(tx.id, tx_id);
    /// }
    /// ```
    transactions: HashMap<TransactionId, TransactionWithState>,
    accounts: HashMap<ClientId, MemAccount>,
}

/// A transaction with its associated current state.
///
/// See [TransactionState] for the possibile states and their meaning.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct TransactionWithState {
    tx: Transaction,
    state: TransactionState,
}

impl TransactionWithState {
    pub const fn valid(tx: Transaction) -> Self {
        Self {
            tx,
            state: TransactionState::Valid,
        }
    }

    pub const fn rejected(tx: Transaction) -> Self {
        Self {
            tx,
            state: TransactionState::Rejected,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum TransactionState {
    /// The transaction is currently valid and its effect is realized.
    ///
    /// The transaction may become `Disputed` if a dispute is claimed.
    Valid,
    /// The transaction is disputed: the corresponding assets are held.
    ///
    /// A transaction dispute can only be claimed by the account owner.
    ///
    /// The transaction can either become `Valid` again following a `resolve` by
    /// the owner or be definitely rejected (and its effects reverted)
    /// following a `chargeback`.
    Disputed,
    /// The transaction was rejected because of insufficient or following a chargeback.
    ///
    /// Once rejected, a transaction stays in the rejected state.
    Rejected,
}

struct MemAccount {
    id: ClientId,
    locked: bool,
    balance: AccountBalance,
}

impl MemAccount {
    pub fn new(id: ClientId) -> Self {
        Self {
            id,
            locked: false,
            balance: AccountBalance::new(),
        }
    }
}

#[derive(Error, Debug, Eq, PartialEq)]
pub enum SubmitError {
    #[error("deposit command failed")]
    Deposit(#[from] DepositError),
    #[error("withdrawal command failed")]
    Withdrawal(#[from] WithdrawalError),
    #[error("dispute command failed")]
    Dispute(#[from] DisputeError),
    #[error("resolve command failed")]
    Resolve(#[from] ResolveError),
    #[error("chargeback command failed")]
    Chargeback(#[from] ChargebackError),
}

#[derive(Error, Debug, Eq, PartialEq)]
pub enum DepositError {
    #[error("multiple different transactions have the same transaction id")]
    TransactionIdConflict,
    #[error("locked client account")]
    Locked,
    #[error("failed to update the account balance due to an overflow or underflow")]
    BalanceUpdateError,
}

#[derive(Error, Debug, Eq, PartialEq)]
pub enum WithdrawalError {
    #[error("multiple different transactions have the same transaction id")]
    TransactionIdConflict,
    #[error("locked client account")]
    Locked,
    #[error("failed to update the account balance due to an overflow or underflow")]
    BalanceUpdateError,
    #[error("insufficient available assets to complete the withdrawal")]
    InsufficientAssets,
}

#[derive(Error, Debug, Eq, PartialEq)]
pub enum DisputeError {
    #[error("disputed transaction #{} not found", .0)]
    NotFound(TransactionId),
    #[error("only the account owner is allowed to claim a dispute: account owner: #{}, claimant: #{}", .owner, .claimant)]
    InvalidClaimant { owner: ClientId, claimant: ClientId },
    #[error("transaction #{} is already rejected", .0)]
    AlreadyRejected(TransactionId),
    #[error("the client account is already locked, cannot submit further disputes")]
    Locked,
    #[error("failed to update the account balance due to an overflow or underflow")]
    BalanceUpdateError,
    #[error("disputing withdrawals is currently denied as per bank policy")]
    WithdrawalDisputeDenied,
    #[error("insufficient available assets to file the dispute")]
    InsufficientAssets,
}

#[derive(Error, Debug, Eq, PartialEq)]
pub enum ResolveError {
    #[error("transaction to resolve (#{}) not found", .0)]
    NotFound(TransactionId),
    #[error("only the account owner is allowed to resolve a dispute claim: account owner: #{}, claimant: #{}", .owner, .claimant)]
    InvalidClaimant { owner: ClientId, claimant: ClientId },
    #[error("transaction #{} is already rejected", .0)]
    AlreadyRejected(TransactionId),
    #[error("the client account is already locked, cannot submit further dispute resolutions")]
    Locked,
    #[error("failed to update the account balance due to an overflow or underflow")]
    BalanceUpdateError,
}

#[derive(Error, Debug, Eq, PartialEq)]
pub enum ChargebackError {
    #[error("transaction to resolve (#{}) not found", .0)]
    NotFound(TransactionId),
    #[error("only the account owner is allowed to resolve a dispute claim: account owner: #{}, claimant: #{}", .owner, .claimant)]
    InvalidClaimant { owner: ClientId, claimant: ClientId },
    #[error("transaction #{} is already rejected", .0)]
    AlreadyRejected(TransactionId),
    #[error("the client account is already locked, cannot submit further dispute resolutions")]
    Locked,
    #[error("failed to update the account balance due to an overflow or underflow")]
    BalanceUpdateError,
}

impl MemAccountService {
    pub fn new(withdrawal_dispute_policy: WithdrawalDisputePolicy) -> Self {
        Self {
            withdrawal_dispute_policy,
            transactions: HashMap::new(),
            accounts: HashMap::new(),
        }
    }

    pub fn submit(&mut self, cmd: Command) -> Result<(), SubmitError> {
        match cmd {
            Command::Deposit(cmd) => self.submit_deposit(cmd)?,
            Command::Withdrawal(cmd) => self.submit_withdrawal(cmd)?,
            Command::Dispute(cmd) => self.submit_dispute(cmd)?,
            Command::Resolve(cmd) => self.submit_resolve(cmd)?,
            Command::Chargeback(cmd) => self.submit_chargeback(cmd)?,
        }
        Ok(())
    }

    pub fn submit_deposit(&mut self, cmd: cmd::Deposit) -> Result<(), DepositError> {
        let cmd = cmd.0;
        let tx = cmd.to_deposit_tx();
        let account = upsert_account(&mut self.accounts, cmd.client);
        let res = upsert_tx(
            &mut self.transactions,
            tx,
            || -> Result<(), DepositError> {
                if account.locked {
                    return Err(DepositError::Locked);
                };

                account
                    .balance
                    .inc_available(cmd.amount)
                    .map_err(|_| DepositError::BalanceUpdateError)?;
                Ok(())
            },
        );

        res.map_err(|e| match e {
            UpsertTxError::Conflict => DepositError::TransactionIdConflict,
            UpsertTxError::Custom(e) => e,
        })
    }

    pub fn submit_withdrawal(&mut self, cmd: cmd::Withdrawal) -> Result<(), WithdrawalError> {
        let cmd = cmd.0;
        let tx = cmd.to_withdrawal_tx();
        let account = upsert_account(&mut self.accounts, cmd.client);
        let res = upsert_tx(
            &mut self.transactions,
            tx,
            || -> Result<(), WithdrawalError> {
                if account.locked {
                    return Err(WithdrawalError::Locked);
                };

                if account.balance.available() < cmd.amount {
                    return Err(WithdrawalError::InsufficientAssets);
                }

                account
                    .balance
                    .dec_available(cmd.amount)
                    .map_err(|_| WithdrawalError::BalanceUpdateError)?;
                Ok(())
            },
        );

        res.map_err(|e| match e {
            UpsertTxError::Conflict => WithdrawalError::TransactionIdConflict,
            UpsertTxError::Custom(e) => e,
        })
    }

    pub fn submit_dispute(&mut self, cmd: cmd::Dispute) -> Result<(), DisputeError> {
        let tx = self
            .transactions
            .get_mut(&cmd.tx)
            .ok_or(DisputeError::NotFound(cmd.tx))?;

        let account = upsert_account(&mut self.accounts, tx.tx.client());

        if cmd.client != account.id {
            return Err(DisputeError::InvalidClaimant {
                owner: account.id,
                claimant: cmd.client,
            });
        }

        if account.locked {
            return Err(DisputeError::Locked);
        }

        match tx.state {
            TransactionState::Rejected => return Err(DisputeError::AlreadyRejected(cmd.tx)),
            TransactionState::Disputed => {
                // Claiming a dispute against the same transaction again is a no-op
            }
            TransactionState::Valid => {
                let disputed_amount = tx.tx.amount();
                let has_more_available_than_disputed =
                    account.balance.available() >= disputed_amount;

                // Check dispute validity
                match tx.tx {
                    Transaction::Deposit(_) => {
                        if !has_more_available_than_disputed {
                            return Err(DisputeError::InsufficientAssets);
                        }
                    }
                    Transaction::Withdrawal(_) => match self.withdrawal_dispute_policy {
                        WithdrawalDisputePolicy::Deny => {
                            return Err(DisputeError::WithdrawalDisputeDenied)
                        }
                        WithdrawalDisputePolicy::IfMoreAvailableThanDisputed => {
                            if !has_more_available_than_disputed {
                                return Err(DisputeError::InsufficientAssets);
                            }
                        }
                    },
                };

                // At this point the dispute is valid: apply it
                account
                    .balance
                    .move_available_to_held(disputed_amount)
                    .map_err(|_| DisputeError::BalanceUpdateError)?;
                tx.state = TransactionState::Disputed;
            }
        };

        Ok(())
    }

    pub fn submit_resolve(&mut self, cmd: cmd::Resolve) -> Result<(), ResolveError> {
        let tx = self
            .transactions
            .get_mut(&cmd.tx)
            .ok_or(ResolveError::NotFound(cmd.tx))?;

        let account = upsert_account(&mut self.accounts, tx.tx.client());

        if cmd.client != account.id {
            return Err(ResolveError::InvalidClaimant {
                owner: account.id,
                claimant: cmd.client,
            });
        }

        if account.locked {
            return Err(ResolveError::Locked);
        }

        match tx.state {
            TransactionState::Rejected => return Err(ResolveError::AlreadyRejected(cmd.tx)),
            TransactionState::Valid => {
                // Resolving a dispute against an already valid transaction is a no-op
            }
            TransactionState::Disputed => {
                let disputed_amount = tx.tx.amount();

                // Un-freeze the held assets by moving them back to the `available` state.
                account
                    .balance
                    .move_held_to_available(disputed_amount)
                    .map_err(|_| ResolveError::BalanceUpdateError)?;
                tx.state = TransactionState::Valid;
            }
        };

        Ok(())
    }

    pub fn submit_chargeback(&mut self, cmd: cmd::Chargeback) -> Result<(), ResolveError> {
        let tx = self
            .transactions
            .get_mut(&cmd.tx)
            .ok_or(ResolveError::NotFound(cmd.tx))?;

        let account = upsert_account(&mut self.accounts, tx.tx.client());

        if cmd.client != account.id {
            return Err(ResolveError::InvalidClaimant {
                owner: account.id,
                claimant: cmd.client,
            });
        }

        if account.locked {
            return Err(ResolveError::Locked);
        }

        match tx.state {
            TransactionState::Rejected => return Err(ResolveError::AlreadyRejected(cmd.tx)),
            TransactionState::Valid => {
                // Resolving a dispute against an already valid transaction is a no-op
            }
            TransactionState::Disputed => {
                let disputed_amount = tx.tx.amount();

                let (new_available, new_held) = match &tx.tx {
                    Transaction::Deposit(_) => {
                        // Remove the disputed amount from the held assets, no change to `available`:
                        let new_held = account
                            .balance
                            .held()
                            .checked_sub(disputed_amount)
                            .ok_or(ResolveError::BalanceUpdateError)?;
                        (account.balance.available(), new_held)
                    }
                    Transaction::Withdrawal(_) => {
                        // Move the held disputed amount to `available`
                        let new_held = account
                            .balance
                            .held()
                            .checked_sub(disputed_amount)
                            .ok_or(ResolveError::BalanceUpdateError)?;
                        let new_available = account
                            .balance
                            .available()
                            .checked_add(disputed_amount)
                            .ok_or(ResolveError::BalanceUpdateError)?;
                        // Then refund the withdrawn assets
                        let new_available = new_available
                            .checked_add(disputed_amount)
                            .ok_or(ResolveError::BalanceUpdateError)?;
                        (new_available, new_held)
                    }
                };

                account
                    .balance
                    .update(new_available, new_held)
                    .map_err(|_| ResolveError::BalanceUpdateError)?;
                account.locked = true;
                tx.state = TransactionState::Rejected;
            }
        };

        Ok(())
    }

    pub fn get_all_accounts(&self) -> MemAccountIter {
        let inner = self.accounts.values();
        MemAccountIter { inner }
    }
}

/// Get or create the account for the provided client
fn upsert_account(
    accounts: &mut HashMap<ClientId, MemAccount>,
    client: ClientId,
) -> &mut MemAccount {
    accounts
        .entry(client)
        .or_insert_with(|| MemAccount::new(client))
}

enum UpsertTxError<E> {
    /// The transaction already exists and does not match the previous value.
    Conflict,
    /// The handler failed with a custom error.
    Custom(E),
}

/// Create a transaction with the provided handler, it it does not already exist.
///
/// If the transaction already exists, the callback is not called and
/// `with_tx` only checks that the transaction matches.
///
/// If the transaction is new, execute the handler. If the handler succeeds,
/// the transaction is marked as valid; otherwise it is rejected.
fn upsert_tx<F, E>(
    transactions: &mut HashMap<TransactionId, TransactionWithState>,
    tx: Transaction,
    handler: F,
) -> Result<(), UpsertTxError<E>>
where
    F: FnOnce() -> Result<(), E>,
{
    let tx_entry = transactions.entry(tx.id());
    let tx_entry = match tx_entry {
        Entry::Occupied(tx_entry) => {
            return if tx_entry.get().tx != tx {
                Err(UpsertTxError::Conflict)
            } else {
                // Same id, with same fields (probably an idempotent retry, ignore)
                Ok(())
            };
        }
        Entry::Vacant(tx_entry) => tx_entry,
    };
    let handler_res = handler();
    match handler_res {
        Ok(()) => {
            tx_entry.insert(TransactionWithState::valid(tx));
            Ok(())
        }
        Err(e) => {
            tx_entry.insert(TransactionWithState::rejected(tx));
            Err(UpsertTxError::Custom(e))
        }
    }
}

impl Default for MemAccountService {
    fn default() -> Self {
        Self::new(WithdrawalDisputePolicy::IfMoreAvailableThanDisputed)
    }
}

pub struct MemAccountIter<'a> {
    inner: std::collections::hash_map::Values<'a, ClientId, MemAccount>,
}

impl<'a> Iterator for MemAccountIter<'a> {
    type Item = Account;

    fn next(&mut self) -> Option<Self::Item> {
        let account = self.inner.next()?;
        Some(Account {
            client: account.id,
            balance: account.balance,
            locked: account.locked,
        })
    }
}
