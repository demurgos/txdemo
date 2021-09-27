use crate::core::{cmd, Account, AccountBalance, ClientId, Command, Transaction, TransactionId};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use thiserror::Error;

pub struct MemAccountService {
    // event_log: Vec<ClientId, MemClient>,
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

#[derive(Copy, Clone, Debug, Eq, Ord, PartialOrd, PartialEq)]
enum TransactionState {
    /// The transaction is currently valid and its effect is realized.
    ///
    /// The transaction may become `Disputed` if a dispute is claimed.
    Valid,
    /// The transaction is disputed: the corresponding assets are held.
    ///
    /// The transaction can either become `Valid` again following a `resolve`
    /// or be definitely rejected (and its effects reverted) following `chargeback`.
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

impl MemAccountService {
    pub fn new() -> Self {
        Self {
            transactions: HashMap::new(),
            accounts: HashMap::new(),
        }
    }

    pub fn submit(&mut self, cmd: Command) -> Result<(), SubmitError> {
        match cmd {
            Command::Deposit(cmd) => self.submit_deposit(cmd)?,
            Command::Withdrawal(cmd) => self.submit_withdrawal(cmd)?,
            _ => todo!(),
        }
        Ok(())
    }

    pub fn submit_deposit(&mut self, cmd: cmd::Deposit) -> Result<(), DepositError> {
        let cmd = cmd.0;
        let tx = cmd.to_deposit_tx();
        let tx_entry = self.transactions.entry(cmd.id);
        let tx_entry = match tx_entry {
            Entry::Occupied(tx_entry) => {
                return if tx_entry.get().tx != tx {
                    Err(DepositError::TransactionIdConflict)
                } else {
                    // Same id, with same fields (probably an idempotent retry, ignore)
                    Ok(())
                };
            }
            Entry::Vacant(tx_entry) => tx_entry,
        };

        let account = upsert_account(&mut self.accounts, cmd.client);
        if account.locked {
            return Err(DepositError::Locked);
        };
        let amount = cmd
            .amount
            .to_signed()
            .map_err(|_| DepositError::BalanceUpdateError)?;
        account
            .balance
            .inc_available(amount)
            .map_err(|_| DepositError::BalanceUpdateError)?;
        tx_entry.insert(TransactionWithState::valid(tx));
        Ok(())
    }

    pub fn submit_withdrawal(&mut self, cmd: cmd::Withdrawal) -> Result<(), WithdrawalError> {
        let cmd = cmd.0;
        let tx = cmd.to_withdrawal_tx();
        let tx_entry = self.transactions.entry(cmd.id);
        let tx_entry = match tx_entry {
            Entry::Occupied(tx_entry) => {
                return if tx_entry.get().tx != tx {
                    Err(WithdrawalError::TransactionIdConflict)
                } else {
                    // Same id, with same fields (probably an idempotent retry, ignore)
                    Ok(())
                };
            }
            Entry::Vacant(tx_entry) => tx_entry,
        };

        let account = upsert_account(&mut self.accounts, cmd.client);
        if account.locked {
            tx_entry.insert(TransactionWithState::rejected(tx));
            return Err(WithdrawalError::Locked);
        };
        let amount = cmd
            .amount
            .to_signed()
            .map_err(|_| WithdrawalError::BalanceUpdateError)?;
        if account.balance.available() < amount {
            tx_entry.insert(TransactionWithState::rejected(tx));
            return Err(WithdrawalError::InsufficientAssets);
        }

        account
            .balance
            .dec_available(amount)
            .map_err(|_| WithdrawalError::BalanceUpdateError)?;
        tx_entry.insert(TransactionWithState::valid(tx));
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

impl Default for MemAccountService {
    fn default() -> Self {
        Self::new()
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
