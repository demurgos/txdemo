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
    transactions: HashMap<TransactionId, Transaction>,
    accounts: HashMap<ClientId, MemAccount>,
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
    #[error("transaction command failed")]
    Transaction(#[from] TransactionError),
}

#[derive(Error, Debug, Eq, PartialEq)]
pub enum TransactionError {
    #[error("multiple different transactions have the same transaction id")]
    TransactionIdConflict,
    #[error("locked client account")]
    Locked,
    #[error("failed to update the account balance due to overflow or underflow")]
    BalanceUpdateError,
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
            _ => todo!(),
        }
        Ok(())
    }

    pub fn submit_deposit(&mut self, cmd: cmd::Deposit) -> Result<(), TransactionError> {
        let cmd = cmd.0;
        let tx = cmd.to_deposit_tx();
        let tx_entry = self.transactions.entry(cmd.id);
        let tx_entry = match tx_entry {
            Entry::Occupied(tx_entry) => {
                return if tx_entry.get() != &tx {
                    Err(TransactionError::TransactionIdConflict)
                } else {
                    // Same id, with same fields (probably an idempotent retry, ignore)
                    Ok(())
                };
            }
            Entry::Vacant(tx_entry) => tx_entry,
        };

        let account = self
            .accounts
            .entry(cmd.client)
            .or_insert_with(|| MemAccount::new(cmd.client));
        if account.locked {
            return Err(TransactionError::Locked);
        };
        let amount = cmd
            .amount
            .to_signed()
            .map_err(|_| TransactionError::BalanceUpdateError)?;
        account
            .balance
            .inc_available(amount)
            .map_err(|_| TransactionError::BalanceUpdateError)?;
        tx_entry.insert(tx);
        Ok(())
    }

    pub fn get_all_accounts(&self) -> MemAccountIter {
        let inner = self.accounts.values();
        MemAccountIter { inner }
    }
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
