mod fixed_decimal;

use crate::core::fixed_decimal::FixedDecimal;
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};
use std::fmt;
use thiserror::Error;

/// Clients are only referenced through their id, a valid `u16`.
///
/// Extended client profiles (name, address, ...) are not the responsibility of
/// this service.
///
/// This also serves as the account id in this lib (we assume there is a 1-1 mapping between
/// accounts and clients).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Deserialize, Serialize)]
pub struct ClientId(u16);

/// Globally unique transaction id for deposits and withdrawals.
///
/// The transaction id is defined by the caller service and must be unique.
/// Submitting two transactions with the same id is supported for idempotence,
/// in this case both transactions must be deeply equal.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Deserialize, Serialize)]
pub struct TransactionId(u32);

#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Deserialize, Serialize,
)]
pub struct UnsignedCurrencyAmount(FixedDecimal<u64, 4>);

impl UnsignedCurrencyAmount {
    pub fn to_signed(self) -> Result<SignedCurrencyAmount, ToSignedCurrencyAmountError> {
        self.try_into()
    }

    pub fn checked_add(self, v: Self) -> Option<Self> {
        self.0.checked_add(&v.0).map(Self)
    }
}

impl fmt::Display for UnsignedCurrencyAmount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Deserialize, Serialize,
)]
pub struct SignedCurrencyAmount(FixedDecimal<i64, 4>);

impl SignedCurrencyAmount {
    pub fn to_unsigned(self) -> Result<UnsignedCurrencyAmount, ToUnsignedCurrencyAmountError> {
        self.try_into()
    }

    pub fn checked_add(self, v: Self) -> Option<Self> {
        self.0.checked_add(&v.0).map(Self)
    }
}

impl fmt::Display for SignedCurrencyAmount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Error, Debug, Eq, PartialEq)]
#[error("conversion error, cannot represent {} as unsigned", .0)]
pub struct ToUnsignedCurrencyAmountError(SignedCurrencyAmount);

impl TryFrom<SignedCurrencyAmount> for UnsignedCurrencyAmount {
    type Error = ToUnsignedCurrencyAmountError;

    fn try_from(value: SignedCurrencyAmount) -> Result<Self, Self::Error> {
        let fractions: i64 = *value.0.fractions();
        let fractions: u64 = fractions
            .try_into()
            .map_err(|_| ToUnsignedCurrencyAmountError(value))?;
        Ok(Self(FixedDecimal::from_fractions(fractions)))
    }
}

#[derive(Error, Debug, Eq, PartialEq)]
#[error("conversion error, cannot represent {} as signed", .0)]
pub struct ToSignedCurrencyAmountError(UnsignedCurrencyAmount);

impl TryFrom<UnsignedCurrencyAmount> for SignedCurrencyAmount {
    type Error = ToSignedCurrencyAmountError;

    fn try_from(value: UnsignedCurrencyAmount) -> Result<Self, Self::Error> {
        let fractions: u64 = *value.0.fractions();
        let fractions: i64 = fractions
            .try_into()
            .map_err(|_| ToSignedCurrencyAmountError(value))?;
        Ok(Self(FixedDecimal::from_fractions(fractions)))
    }
}

/// A deposit transaction.
///
/// If the client account is not frozen, add funds to it.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct TransactionMeta {
    pub id: TransactionId,
    pub client: ClientId,
    pub amount: UnsignedCurrencyAmount,
}

impl TransactionMeta {
    pub fn to_deposit_tx(self) -> Transaction {
        Transaction::Deposit(self)
    }

    pub fn to_withdrawal_tx(self) -> Transaction {
        Transaction::Withdrawal(self)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Transaction {
    /// Add an amount of currency to a non-frozen account.
    Deposit(TransactionMeta),
    /// Remove an amount of currency from a non-frozen account.
    Withdrawal(TransactionMeta),
}

impl Transaction {
    pub const fn id(&self) -> TransactionId {
        self.meta().id
    }

    const fn meta(&self) -> &TransactionMeta {
        match self {
            Self::Deposit(ref tx) => tx,
            Self::Withdrawal(ref tx) => tx,
        }
    }
}

/// A deposit transaction.
///
/// If the client account is not frozen, add funds to it.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct Account {
    /// Reference to the client owning this account
    pub client: ClientId,
    pub balance: AccountBalance,
    pub locked: bool,
}

/// Current balance of an account
///
/// The balance is defined by the following two kinds of assets:
/// - Available: Non-frozen account can use/withdraw this amount
/// - Held: Amount corresponding to a currently disputed transaction
///
/// The balance also allows to retrieve the total amount associated with the
/// account. The total is always the sum of the available and held amounts.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct AccountBalance {
    available: SignedCurrencyAmount,
    held: SignedCurrencyAmount,
    // The total is materialized to make sure it is always representable
    // (so there is no overflow/underflow when summing the other fields.
    total: SignedCurrencyAmount,
}

#[derive(Error, Debug, Eq, PartialEq)]
#[error("failed to update balance due to overflow or underflow")]
pub struct BalanceUpdateError;

impl AccountBalance {
    pub fn new() -> Self {
        Self {
            available: SignedCurrencyAmount::default(),
            held: SignedCurrencyAmount::default(),
            total: SignedCurrencyAmount::default(),
        }
    }

    /// Get the current available (non-disputed) amount of currency
    pub fn available(self) -> SignedCurrencyAmount {
        self.available
    }

    /// Get the amount of currency currently held due to a dispute
    pub fn held(self) -> SignedCurrencyAmount {
        self.held
    }

    /// Get the total amount of currency
    pub fn total(self) -> SignedCurrencyAmount {
        self.total
    }

    /// Increment the `available` value by the provided amount
    ///
    /// Errors if the update causes an underflow/overflow
    pub fn inc_available(
        &mut self,
        amount: SignedCurrencyAmount,
    ) -> Result<(), BalanceUpdateError> {
        self.available = self
            .available
            .checked_add(amount)
            .ok_or(BalanceUpdateError)?;
        self.total = self.total.checked_add(amount).ok_or(BalanceUpdateError)?;
        Ok(())
    }

    /// Increment the `held` value by the provided amount
    ///
    /// Errors if the update causes an underflow/overflow
    pub fn inc_held(&mut self, amount: SignedCurrencyAmount) -> Result<(), BalanceUpdateError> {
        self.held = self.held.checked_add(amount).ok_or(BalanceUpdateError)?;
        self.total = self.total.checked_add(amount).ok_or(BalanceUpdateError)?;
        Ok(())
    }
}

impl Default for AccountBalance {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum Command {
    Deposit(cmd::Deposit),
    Withdrawal(cmd::Withdrawal),
    Dispute(cmd::Dispute),
    Resolve(cmd::Resolve),
    Chargeback(cmd::Chargeback),
}

pub mod cmd {
    use crate::core::{ClientId, TransactionId, TransactionMeta};

    #[derive(Debug, Eq, PartialEq)]
    pub struct Deposit(pub TransactionMeta);

    #[derive(Debug, Eq, PartialEq)]
    pub struct Withdrawal(pub TransactionMeta);

    #[derive(Debug, Eq, PartialEq)]
    pub struct Dispute {
        /// Client claiming that a previous transaction was erroneous.
        client: ClientId,
        tx: TransactionId,
    }

    #[derive(Debug, Eq, PartialEq)]
    pub struct Resolve {
        /// Client settling the dispute as resolved.
        client: ClientId,
        tx: TransactionId,
    }

    #[derive(Debug, Eq, PartialEq)]
    pub struct Chargeback {
        /// Client settling the dispute with chargeback.
        client: ClientId,
        tx: TransactionId,
    }
}
