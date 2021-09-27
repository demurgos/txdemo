# TxDemo

_TxDemo_ is a small Rust CLI application showcasing a minimal transaction processing service.

In this model, clients can deposit and withdraw assets from their unique account. It is also possible to dispute
deposits and withdrawals (mark them as erroneous). While a transaction is disputed, the corresponding assets are held
until the dispute is settled with either a resolution (ignore the dispute) or a chargeback (cancel the disputed
transaction).

## Getting started

This is a regular Rust project. You can run it with:

```
cargo run -- [flags] [input]
```

The program takes a single optional arguments: the path to a CSV file containing
the commands to run. If the path is missing, the application reads from stdin.

The program prints the final state of all the accounts to the standard output.

Flags:
- `--sort`: Sort the output by client id.
- `--deny-withdrawal-dispute`: Prevent `dispute` commands on `withdrawal` transactions.

**Example**:

**TODO**

```
$ cat transactions.csv
$ cargo run -- transactions.csv > accounts.csv
$ cat accounts.csv
```

## Introduction

Clients are automatically created with an empty account the first time they
are mentioned. Client accounts contain `available` and `held` assets.
`available` assets can be freely withdrawn, `held` assets are currently
dispute and can't be used until the dispute is settled. If the dispute is
settled with a chargeback (cancel the transaction), the account it locked forever
and any modification to its state is prevented (new transactions, disputes,
resolutions, or chargebacks).

To prevent abuses, both `available` and `held` client assets must always be
positive (or zero). This invariant is checked by Rust's type system.

## Commands

This section documents the five supported commands:

- `deposit`: Create a new transaction to increase the available assets of the account.
- `withdrawal`: Create a new transaction to decrease the available assets of the account.
- `dispute`: File a dispute againts a transaction, freezing its assets in the `held` state until the dispute is settled.
- `resolve`: Settle a dispute by cancelling the dispute: the assets are released back to the `available` state.
- `chargeback`: Settle a dispute by reverting the transaction. The account is locked.

### deposit

If the client account is not locked, increase its `available` assets by
the provided amount.

#### Example

- Old state

  ```
  client, available, held, total, locked
       1,        10,    1,    11, false
  ```

- Commands

  ```
     type, client, tx, amount
  deposit,      1,  1,      3
  ```
  
- New state

  ```
  client, available, held, total, locked
       1,        13,    1,    14, false
  ```

### withdrawal

If the client account is not locked and has sufficient availabl assets, decrease
its `available` assets by the provided amount.

#### Example

- Old state

  ```
  client, available, held, total, locked
       1,        10,    1,    11, false
  ```

- Commands

  ```
        type, client, tx, amount
  withdrawal,      1,  1,      3
  ```

- New state

  ```
  client, available, held, total, locked
       1,         7,    1,     8, false
  ```

### dispute

- **type**: `"dispute"`
- **client**: `ClientId`, the client claiming that the transaction is erroneous
- **tx**: `TransactionId`, id of the disputed transaction
- **amount**: empty

If the transaction exist, the corresponding account is not locked and the
claimant client is the one who did the transaction, mark the transaction as
disputed.

A dispute can be filed for any passed transaction (there is no time limit)
as long as the account has enough `available` assets.

If the disputed transaction is a deposit, the situation is simple: move the
erroneous assets to the `held` state until the dispute is settled.

If the disputed transaction is a withdrawal, the situation is more complex as
a chargeback would mean refunding the client, and we want to prevent abuses.
This app provides two options, you can pick the preferred one with the
flag `--deny-withdrawal-dispute`:
- Default (no flag): Allow withdrawal disputes as long as the account still has
  at least the same amount of assets available. This means that an account
  with `4.0` remaining available assets can dispute a `3.0` withdrawal but not
  a `5.0` withdrawal. This strategy means that a refund will never pay out more
  than the available assets. If it turns out that this was a dispute trying to
  scam the bank, we can seize the locked account and there will always be enough
  assets to pay back the refund.
- `--deny-withdrawal-dispute`: Simply prevent any dispute regarding withdrawals.
  Safer for the bank as it is almost impossible to abuse, but it may hurt honest
  clients.

#### Example - Dispute deposit

- Old state

  ```
  client, available, held, total, locked
       1,        10,    1,    11, false
  ```

- Commands

  ```
        type, client, tx, amount
     deposit,      1,  1,      3
     dispute,      1,  1,
  ```

- New state

  ```
  client, available, held, total, locked
       1,        10,    4,    14, false
  ```

#### Example - Dispute withdrawal

- Old state

  ```
  client, available, held, total, locked
       1,        10,    1,    11, false
  ```

- Commands

  ```
        type, client, tx, amount
  withdrawal,      1,  1,      3
     dispute,      1,  1,
  ```

- New state

  ```
  client, available, held, total, locked
       1,         4,    4,     8, false
  ```

### resolve

Cancel a previous dispute and restore the corresponding held assets to the
`available` state.

### chargeback

Cancel the dispute transactions (refunding the account if needed).
The account is immediately locked following a chargeback, allowing the bank
to further investigate the situation. If it turns out that this was a fraud,
the account will still have the refunded assets so no abuse is possible
this way.

# License

[AGPL-3.0-or-later](./LICENSE.md)
