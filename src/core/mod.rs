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

impl fmt::Display for ClientId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Globally unique transaction id for deposits and withdrawals.
///
/// The transaction id is defined by the caller service and must be unique.
/// Submitting two transactions with the same id is supported for idempotence,
/// in this case both transactions must be deeply equal.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Deserialize, Serialize)]
pub struct TransactionId(u32);

impl fmt::Display for TransactionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// An unsigned amount of assets.
///
/// This type is backed by a [FixedDecimal<u64, 4>]:
/// - Minimum value: `0.0000`
/// - Maximum value: `1844674407370955.1615` (≃1.8e15)
/// - Precision: `0.0001`
#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Deserialize, Serialize,
)]
pub struct UnsignedAssetCount(FixedDecimal<u64, 4>);

impl UnsignedAssetCount {
    pub fn to_signed(self) -> Result<SignedAssetCount, ToSignedCurrencyAmountError> {
        self.try_into()
    }

    pub fn checked_add(self, v: Self) -> Option<Self> {
        self.0.checked_add(&v.0).map(Self)
    }

    pub fn checked_sub(self, v: Self) -> Option<Self> {
        self.0.checked_sub(&v.0).map(Self)
    }
}

impl fmt::Display for UnsignedAssetCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Deserialize, Serialize,
)]
pub struct SignedAssetCount(FixedDecimal<i64, 4>);

impl SignedAssetCount {
    pub fn to_unsigned(self) -> Result<UnsignedAssetCount, ToUnsignedCurrencyAmountError> {
        self.try_into()
    }

    pub fn checked_add(self, v: Self) -> Option<Self> {
        self.0.checked_add(&v.0).map(Self)
    }

    pub fn checked_sub(self, v: Self) -> Option<Self> {
        self.0.checked_sub(&v.0).map(Self)
    }
}

impl fmt::Display for SignedAssetCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Error, Debug, Eq, PartialEq)]
#[error("conversion error, cannot represent {} as unsigned", .0)]
pub struct ToUnsignedCurrencyAmountError(SignedAssetCount);

impl TryFrom<SignedAssetCount> for UnsignedAssetCount {
    type Error = ToUnsignedCurrencyAmountError;

    fn try_from(value: SignedAssetCount) -> Result<Self, Self::Error> {
        let fractions: i64 = *value.0.fractions();
        let fractions: u64 = fractions
            .try_into()
            .map_err(|_| ToUnsignedCurrencyAmountError(value))?;
        Ok(Self(FixedDecimal::from_fractions(fractions)))
    }
}

#[derive(Error, Debug, Eq, PartialEq)]
#[error("conversion error, cannot represent {} as signed", .0)]
pub struct ToSignedCurrencyAmountError(UnsignedAssetCount);

impl TryFrom<UnsignedAssetCount> for SignedAssetCount {
    type Error = ToSignedCurrencyAmountError;

    fn try_from(value: UnsignedAssetCount) -> Result<Self, Self::Error> {
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
    pub amount: UnsignedAssetCount,
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

    pub const fn client(&self) -> ClientId {
        self.meta().client
    }

    pub const fn amount(&self) -> UnsignedAssetCount {
        self.meta().amount
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
///
/// This struct enforces that both the `available` and `held` assets are
/// always positive. It also prevents any updated that would cause an
/// overflow or underflow of the `available`, `held` or `total` values.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct AccountBalance {
    available: UnsignedAssetCount,
    held: UnsignedAssetCount,
}

#[derive(Error, Debug, Eq, PartialEq)]
#[error("failed to update balance due to overflow or underflow")]
pub struct BalanceUpdateError;

impl AccountBalance {
    pub fn new() -> Self {
        Self {
            available: UnsignedAssetCount::default(),
            held: UnsignedAssetCount::default(),
        }
    }

    /// Get the current available (non-disputed) amount of currency
    pub fn available(self) -> UnsignedAssetCount {
        self.available
    }

    /// Get the amount of currency currently held due to a dispute
    pub fn held(self) -> UnsignedAssetCount {
        self.held
    }

    /// Get the total amount of currency
    pub fn total(self) -> UnsignedAssetCount {
        self.available
            .checked_add(self.held)
            .expect("internal invariant should enforce that computing the total always succeeds")
    }

    /// Increment the `available` value by the provided amount
    ///
    /// Errors if the update causes an underflow/overflow
    ///
    /// This update is atomic.
    pub fn inc_available(&mut self, amount: UnsignedAssetCount) -> Result<(), BalanceUpdateError> {
        let new_available = self
            .available
            .checked_add(amount)
            .ok_or(BalanceUpdateError)?;
        self.update(new_available, self.held)
    }

    /// Move assets from the `available` to the `held` state.
    ///
    /// Decrements `available` and increments `held` by the provided amount.
    ///
    /// Errors if the update causes an underflow/overflow
    ///
    /// This update is atomic.
    pub fn move_available_to_held(
        &mut self,
        amount: UnsignedAssetCount,
    ) -> Result<(), BalanceUpdateError> {
        let new_available = self
            .available
            .checked_sub(amount)
            .ok_or(BalanceUpdateError)?;
        let new_held = self.held.checked_add(amount).ok_or(BalanceUpdateError)?;
        self.update(new_available, new_held)
    }

    /// Decrement the `available` value by the provided amount
    ///
    /// Errors if the update causes an underflow/overflow
    ///
    /// This update is atomic.
    pub fn dec_available(&mut self, amount: UnsignedAssetCount) -> Result<(), BalanceUpdateError> {
        let new_available = self
            .available
            .checked_sub(amount)
            .ok_or(BalanceUpdateError)?;
        self.update(new_available, self.held)
    }

    /// Perform an atomic update of the account balance.
    ///
    /// The update fails if it causes any overflow or underflow.
    fn update(
        &mut self,
        new_available: UnsignedAssetCount,
        new_held: UnsignedAssetCount,
    ) -> Result<(), BalanceUpdateError> {
        let total_sum_is_safe_to_compute = new_available.checked_add(new_held).is_some();
        if !total_sum_is_safe_to_compute {
            return Err(BalanceUpdateError);
        }
        self.available = new_available;
        self.held = new_held;
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
        pub client: ClientId,
        pub tx: TransactionId,
    }

    #[derive(Debug, Eq, PartialEq)]
    pub struct Resolve {
        /// Client settling the dispute as resolved.
        pub client: ClientId,
        pub tx: TransactionId,
    }

    #[derive(Debug, Eq, PartialEq)]
    pub struct Chargeback {
        /// Client settling the dispute with chargeback.
        pub client: ClientId,
        pub tx: TransactionId,
    }
}
