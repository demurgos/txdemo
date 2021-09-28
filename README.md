# TxDemo

_TxDemo_ is a small Rust CLI application showcasing a minimal transaction processing service.

In this model, clients can deposit and withdraw assets from their unique account. It is also possible to dispute
deposits and withdrawals (mark them as erroneous). While a transaction is disputed, the corresponding assets are held
until the dispute is settled with either a resolution (ignore the dispute) or a chargeback (cancel the disputed
transaction).

# Getting started

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

# Introduction

Clients are automatically created with an empty account the first time they
are mentioned. Client accounts contain `available` and `held` assets.
`available` assets can be freely withdrawn, `held` assets are currently
dispute and can't be used until the dispute is settled. If the dispute is
settled with a chargeback (cancel the transaction), the account it locked forever
and any modification to its state is prevented (new transactions, disputes,
resolutions, or chargebacks).

To prevent abuses, both `available` and `held` client assets must always be
positive (or zero). This invariant is checked by Rust's type system.

# Commands

This section documents the five supported commands:

- `deposit`: Create a new transaction to increase the available assets of the account.
- `withdrawal`: Create a new transaction to decrease the available assets of the account.
- `dispute`: File a dispute againts a transaction, freezing its assets in the `held` state until the dispute is settled.
- `resolve`: Settle a dispute by cancelling the dispute: the assets are released back to the `available` state.
- `chargeback`: Settle a dispute by reverting the transaction. The account is locked.

## deposit

If the client account is not locked, increase its `available` assets by
the provided amount.

### Example

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

## withdrawal

If the client account is not locked and has sufficient availabl assets, decrease
its `available` assets by the provided amount.

### Example

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

## dispute

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

### Example - Dispute deposit

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

### Example - Dispute withdrawal

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

## resolve

Cancel a previous dispute and restore the corresponding held assets to the
`available` state.

## chargeback

Cancel the dispute transactions (refunding the account if needed).
The account is immediately locked following a chargeback, allowing the bank
to further investigate the situation. If it turns out that this was a fraud,
the account will still have the refunded assets so no abuse is possible
this way.

# Project management

Besides `cargo run`, the following commands are relevant to this project.
This repository is configured to run most checks in CI.

## Test

```
cargo test
```

## Test with coverage

On Linux, install [Tarpaulin](https://github.com/xd009642/tarpaulin) and run the
following command:

```
cargo tarpaulin --out html --output-dir ./target/debug/coverage/
```

You may also use [Grcov](https://github.com/mozilla/grcov) to measure code coverage.
It requires Nightly but is more acurate in my experience:

```
rustup run nightly cargo install grcov
rustup run nightly rustup component add llvm-tools-preview
rustup run nightly cargo build
rm -rf target/debug/profile/
mkdir -p target/debug/profile/
LLVM_PROFILE_FILE="target/debug/profile/%p-%m.profraw" RUSTFLAGS="-Zinstrument-coverage" rustup run nightly cargo test
rustup run nightly grcov ./target/debug/profile/ -s . --binary-path ./target/debug/ -t html --branch --ignore-not-existing -o ./target/debug/coverage/
```

The report will be in `./target/debug/coverage/`.


## Format code

Make sure you enabled the `rustmt` component on your toolchain, then run:

```
cargo fmt --all
```

To only check the code, use:

```
cargo fmt --all -- --check
```

## Lint

Make sure you enabled the `clippy` component on your toolchain, then run:

```
cargo clippy --all-targets --all-features -- -D warnings
```

## Audit

This project is checked with [cargo-audit](https://github.com/RustSec/rustsec/tree/main/cargo-audit).
You can run the check yourself with:

```
cargo audit
```

# Correctness

Two of the most invariants maintained by this crate are:
- Both the `available` and `held` assets of an account must always be positive.
- There should be exactly 4 digits of precision, without precision loss.

These invariants are enforced by the type system so they can never be broken
if the code contains a business logic error.

This is solved by representing assets amount with a fixed-point decimal number.
There are some existing libraries to handle such cases, but they were missing
some guarantees I wanted so I wrote my own module. It lets me define the type
`FixedDecimal<u64, 4>`: a fixed-point decimal number representing a `u64` amount
of fractions each corresponding to `1-e4`. Using this type naturally prevents
negative asset counts.

An important aspect of my module is that it requires checked arithmetic and
prevents rounding. It means that parsing an input value of `0.12345` is rejected
because representing it would require rounding (and rounding requires more
information). Enforced checked arithmetic means that a business logic error
causing an underflow is impossible; for example subtracting more assets than
available will not cause the balance to get really high.

Apart from this, the code should be readable and commented enough to help spot
mistakes.

This project does not use any unsafe code itself, and relies on a small
number of established dependencies.

# Reliability

This project tries its best to avoid panics and handle errors gracefully.
In particular, it recovers from errors caused by invalid commands and continues
to handle valid commands.

This projects uses Github Actions as a CI service to check commits and prevent
regressions. It also uses `rustfmt` and `clippy` to keep the code readable and
prevent common mistakes.

The project is extensively tested and has a high code coverage ratio. Rust's
code coverage ecosystem is still immature but grcov reports at least 90%
of code coverage (98% for the module containing the business logic).

The tests are defined as directories in `./test-ressources`. Each test
contains an input file, an expected output file and an optional `flags.txt` to
run the program with extra flags. The program executed against the input file
and the actual output and errors saved (so they can be easily reviewed). The
test passes if the actual output exactly matches the expected output.
Note that the tests enforce the `--sort` flag for determinism.

# Performance

The `FixedDecimal` type was written to be correct and flexible, performance was
a secondary concern, in particular regarding parsing and formatting. This type
can be optimized without much change to its API if it becomes a bottleneck.

# License

[AGPL-3.0-or-later](./LICENSE.md)
