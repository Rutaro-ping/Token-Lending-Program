## **This project is forked from https://github.com/solana-labs/solana-program-library to showcase a bug on *creating ObligationLiquidity* https://github.com/yanCode/solana-token-lending/issues/2**




# Token Lending Program

## Overview

This project implements a lending protocol for Solana blockchain, forked from [solana-program-library/token-lending](https://github.com/joncinque/solana-program-library/tree/master/token-lending). 

## Notable Changes

This project includes several significant modifications and enhancements over the original implementation:

1. **Collateral Rate Calculation Bug**: as the original repo is marked as archived, I have to fix bug in this repo, and described it it.

2. **Add Stateful Tests**: In `program/test/intergration.rs`, and its dependent methods inside `program/tests/stateful`. I provided two stateful tests: One is Alice can borrow tokens and then successfully repay. Another test case is Alice can borrow tokens but fail to repay, so Bob come to liquidate the collateral of Alice. These stateful tests can help the developer to understand the business logic better, also it helps to find bugs as described in [1. **Collateral rate calculation Bug**](#collateral-rate-calculation-bug).

3. **Improved Error Handling**: to handle the error, the original logic was like this:
    ```rust
    self.liquidity
        .compound_interest(current_borrow_rate, slots_elapsed)?;
    ```
    Which using `?` to propagate the error, but errors from `compound_interest` are very generic, collectively named as `LendingError::MathOverflow`, which is not helpful what particular input makes the error.
    as rust solana program doesn't provide stack trace, so I use `map_err` to tap the error message with details:
    ```rust
    self.liquidity
      .compound_interest(current_borrow_rate, slots_elapsed)
      .map_err(|e| {
        debug_msg!(
            "Error in accrue_interest:, current_borrow_rate: {}, slots_elapsed: {}",
            current_borrow_rate,
            slots_elapsed
         );
        e
          })?;
    ```
    Also `debug_msg!` is a self-defined macro that only prints the error message in `test-sbf` and `testnet`. *Note*: avoid printing the log in `mainnet` is very important, because the log will be stored in blockchain permanently, which can increase the costs. 
    **_Most importantly_**, too detailed log may help malicious hackers pinpoint possible loopholes of system.


5. **Other Trivial Changes**: 
- Refactored many chunky `rs` files into modular architecture. As an example `program/src/state/`
- Cleared out sysvar accounts like `Rent` and `Clock`, which were required by old versions of solana, but since in new versions they can be populated by the program itself.
- Add `assert_is_signer!` to reduce boilerplate code of validating if an account is a signer, much like what Anchor does.
- In many test cases, it used `&mut banks_client`, but in the logic it doesn't mutate the state at all, so I changed it to `&banks_client`.otherwise it would cause compile error when calling a `fn` from another `fn` while both used mutable reference.
- refactored converting a decimal number to a whole number, using `DECIMALS_LOOKUP` array, which reduces the time complexity from `O(log(n))` to `O(1)`.

## Usage

 To run all tests:
   ```bash
   cargo test-sbf --features test-sbf
   ```

To run a specific test, use a format like this :
   ```bash
   cargo test-sbf --package spl-token-lending --test intergration --features test-sbf -- alice_can_borrow_sol_and_repay --exact --show-output
   ```
Note: Add `--features test-sbf` before positional arguments.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
