mod helpers;
use {
    helpers::{add_lending_market, add_sol_oracle},
    solana_program_test::*,
    solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer},
    spl_token_lending::processor::process_instruction,
};

#[tokio::test]
async fn test_success() {
    let mut test = ProgramTest::new(
        "spl_token_lending",
        spl_token_lending::id(),
        processor!(process_instruction),
    );
    // limit to track compute unit increase
    test.set_compute_max_units(70_000);
    let user_accounts_owner = Keypair::new();
    let lending_market = add_lending_market(&mut test);
    let sol_oracle = add_sol_oracle(&mut test);
    let (mut banks_client, payer, _recent_blockhash) = test.start().await;
}

