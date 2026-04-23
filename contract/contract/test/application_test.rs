#![cfg(test)]

use crate::{
    base::{errors::CrowdfundingError, types::{ApplicationStatus, PoolConfig}},
    crowdfunding::{CrowdfundingContract, CrowdfundingContractClient},
};
use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, String};

fn setup(env: &Env) -> (CrowdfundingContractClient<'_>, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(CrowdfundingContract, ());
    let client = CrowdfundingContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    client.initialize(&admin, &token_address, &0);
    (client, admin, token_address)
}

fn create_pool(env: &Env, client: &CrowdfundingContractClient<'_>, token_address: &Address) -> u64 {
    let creator = Address::generate(env);
    let config = PoolConfig {
        name: String::from_str(env, "Scholarship Fund"),
        description: String::from_str(env, "Fund for student scholarships"),
        target_amount: 100_000i128,
        min_contribution: 0,
        is_private: false,
        duration: 30 * 24 * 60 * 60,
        created_at: env.ledger().timestamp(),
        token_address: token_address.clone(),
    };

    client.create_pool(&creator, &config)
}

#[test]
fn test_apply_for_scholarship_success() {
    let env = Env::default();
    let (client, _, token_address) = setup(&env);

    let pool_id = create_pool(&env, &client, &token_address);
    let applicant = Address::generate(&env);
    let credentials = Bytes::from_array(&env, &[1, 2, 3, 4]);

    client.apply_for_scholarship(&pool_id, &applicant, &credentials);

    let application = client.get_application(&pool_id, &applicant);
    assert_eq!(application.status, ApplicationStatus::Pending);
    assert_eq!(application.pool_id, pool_id);
}

#[test]
fn test_approve_application_changes_status() {
    let env = Env::default();
    let (client, _, token_address) = setup(&env);

    let pool_id = create_pool(&env, &client, &token_address);
    let applicant = Address::generate(&env);
    let validator = Address::generate(&env);
    let credentials = Bytes::from_array(&env, &[5, 6, 7]);

    client.apply_for_scholarship(&pool_id, &applicant, &credentials);
    client.approve_application(&pool_id, &applicant, &validator, &Some(String::from_str(&env, "Approved")));

    let application = client.get_application(&pool_id, &applicant);
    assert_eq!(application.status, ApplicationStatus::Approved);
    assert_eq!(application.reviewer.unwrap(), validator);
}

#[test]
fn test_reject_application_changes_status() {
    let env = Env::default();
    let (client, _, token_address) = setup(&env);

    let pool_id = create_pool(&env, &client, &token_address);
    let applicant = Address::generate(&env);
    let validator = Address::generate(&env);
    let credentials = Bytes::from_array(&env, &[9, 10, 11]);

    client.apply_for_scholarship(&pool_id, &applicant, &credentials);
    client.reject_application(&pool_id, &applicant, &validator, &Some(String::from_str(&env, "Rejected")));

    let application = client.get_application(&pool_id, &applicant);
    assert_eq!(application.status, ApplicationStatus::Rejected);
    assert_eq!(application.reviewer.unwrap(), validator);
}

#[test]
fn test_apply_for_scholarship_empty_credentials_fails() {
    let env = Env::default();
    let (client, _, token_address) = setup(&env);

    let pool_id = create_pool(&env, &client, &token_address);
    let applicant = Address::generate(&env);
    let credentials = Bytes::from_array(&env, &[]);

    let result = client.try_apply_for_scholarship(&pool_id, &applicant, &credentials);
    assert_eq!(result, Err(Ok(CrowdfundingError::InvalidApplicationCredentials)));
}

#[test]
fn test_claim_funds_success() {
    let env = Env::default();
    let (client, _, token_address) = setup(&env);

    let pool_id = create_pool(&env, &client, &token_address);
    let student = Address::generate(&env);
    let validator = Address::generate(&env);
    let credentials = Bytes::from_array(&env, &[1, 2, 3]);

    // Apply and approve application
    client.apply_for_scholarship(&pool_id, &student, &credentials);
    client.approve_application(&pool_id, &student, &validator, &None);

    // Fund the pool
    let sponsor = Address::generate(&env);
    let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token_address);
    token_client.mint(&sponsor, &1000i128);
    client.contribute(&pool_id, &sponsor, &token_address, &1000i128, &false);

    // Claim funds
    let claim_amount = 500i128;
    client.claim_funds(&student, &pool_id, &claim_amount);

    // Verify balances
    assert_eq!(token_client.balance(&student), claim_amount);
    assert_eq!(token_client.balance(&client.address), 1000i128 - claim_amount);
}

#[test]
fn test_claim_funds_rejected_application_fails() {
    let env = Env::default();
    let (client, _, token_address) = setup(&env);

    let pool_id = create_pool(&env, &client, &token_address);
    let student = Address::generate(&env);
    let validator = Address::generate(&env);
    let credentials = Bytes::from_array(&env, &[1, 2, 3]);

    // Apply and reject application
    client.apply_for_scholarship(&pool_id, &student, &credentials);
    client.reject_application(&pool_id, &student, &validator, &None);

    // Try to claim funds
    let result = client.try_claim_funds(&student, &pool_id, &100i128);
    assert_eq!(result, Err(Ok(CrowdfundingError::Unauthorized)));
}

#[test]
fn test_claim_funds_pending_application_fails() {
    let env = Env::default();
    let (client, _, token_address) = setup(&env);

    let pool_id = create_pool(&env, &client, &token_address);
    let student = Address::generate(&env);
    let credentials = Bytes::from_array(&env, &[1, 2, 3]);

    // Apply but don't approve
    client.apply_for_scholarship(&pool_id, &student, &credentials);

    // Try to claim funds
    let result = client.try_claim_funds(&student, &pool_id, &100i128);
    assert_eq!(result, Err(Ok(CrowdfundingError::Unauthorized)));
}

#[test]
fn test_claim_funds_invalid_amount_fails() {
    let env = Env::default();
    let (client, _, token_address) = setup(&env);

    let pool_id = create_pool(&env, &client, &token_address);
    let student = Address::generate(&env);
    let validator = Address::generate(&env);
    let credentials = Bytes::from_array(&env, &[1, 2, 3]);

    // Apply and approve application
    client.apply_for_scholarship(&pool_id, &student, &credentials);
    client.approve_application(&pool_id, &student, &validator, &None);

    // Try to claim invalid amount
    let result = client.try_claim_funds(&student, &pool_id, &0i128);
    assert_eq!(result, Err(Ok(CrowdfundingError::InvalidAmount)));
}

#[test]
fn test_claim_funds_overdraw_fails() {
    let env = Env::default();
    let (client, _, token_address) = setup(&env);

    let pool_id = create_pool(&env, &client, &token_address);
    let student = Address::generate(&env);
    let validator = Address::generate(&env);
    let credentials = Bytes::from_array(&env, &[1, 2, 3]);

    // Apply and approve application
    client.apply_for_scholarship(&pool_id, &student, &credentials);
    client.approve_application(&pool_id, &student, &validator, &None);

    // Fund the pool with limited amount
    let sponsor = Address::generate(&env);
    let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token_address);
    token_client.mint(&sponsor, &100i128);
    client.contribute(&pool_id, &sponsor, &token_address, &100i128, &false);

    // Try to claim more than available
    let result = client.try_claim_funds(&student, &pool_id, &200i128);
    assert_eq!(result, Err(Ok(CrowdfundingError::InsufficientBalance)));
}

#[test]
fn test_claim_funds_partial_claims() {
    let env = Env::default();
    let (client, _, token_address) = setup(&env);

    let pool_id = create_pool(&env, &client, &token_address);
    let student = Address::generate(&env);
    let validator = Address::generate(&env);
    let credentials = Bytes::from_array(&env, &[1, 2, 3]);

    // Apply and approve application
    client.apply_for_scholarship(&pool_id, &student, &credentials);
    client.approve_application(&pool_id, &student, &validator, &None);

    // Fund the pool
    let sponsor = Address::generate(&env);
    let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token_address);
    token_client.mint(&sponsor, &1000i128);
    client.contribute(&pool_id, &sponsor, &token_address, &1000i128, &false);

    // First claim
    client.claim_funds(&student, &pool_id, &300i128);
    assert_eq!(token_client.balance(&student), 300i128);

    // Second claim
    client.claim_funds(&student, &pool_id, &400i128);
    assert_eq!(token_client.balance(&student), 700i128);

    // Third claim exceeds remaining balance
    let result = client.try_claim_funds(&student, &pool_id, &400i128);
    assert_eq!(result, Err(Ok(CrowdfundingError::InsufficientBalance)));
}
