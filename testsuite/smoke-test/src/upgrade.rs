// Copyright (c) Aptos
// SPDX-License-Identifier: Apache-2.0

use crate::aptos_cli::validator::init_validator_account;
use crate::{
    aptos::move_test_helpers, smoke_test_environment::SwarmBuilder,
    test_utils::check_create_mint_transfer, workspace_builder, workspace_builder::workspace_root,
};
use aptos::move_tool::ArgWithType;
use aptos_crypto::ValidCryptoMaterialStringExt;
use aptos_forge::Swarm;
use aptos_gas::{AptosGasParameters, GasQuantity, InitialGasSchedule, ToOnChainGasSchedule};
use aptos_keygen::KeyGen;
use aptos_release_builder::components::{
    feature_flags::{FeatureFlag, Features},
    gas::generate_gas_upgrade_proposal,
};
use aptos_temppath::TempPath;
use std::borrow::Borrow;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use std::{fs, thread};

#[tokio::test]
/// This test verifies the flow of aptos framework upgrade process.
/// i.e: The network will be alive after applying the new aptos framework release.
async fn test_upgrade_flow() {
    // prebuild tools.
    let aptos_cli = workspace_builder::get_bin("aptos");

    let num_nodes = 5;
    let (mut env, _cli, _) = SwarmBuilder::new_local(num_nodes)
        .with_aptos_testnet()
        .build_with_cli(0)
        .await;

    let url = env.aptos_public_info().url().to_string();
    let private_key = env
        .aptos_public_info()
        .root_account()
        .private_key()
        .to_encoded_string()
        .unwrap();

    // Bump the limit in gas schedule
    // TODO: Replace this logic with aptos-gas
    let mut gas_parameters = AptosGasParameters::initial();
    gas_parameters.txn.max_transaction_size_in_bytes = GasQuantity::new(100_000_000);

    let gas_schedule = aptos_types::on_chain_config::GasScheduleV2 {
        feature_version: aptos_gas::LATEST_GAS_FEATURE_VERSION,
        entries: gas_parameters.to_on_chain_gas_schedule(),
    };

    let (_, update_gas_script) =
        generate_gas_upgrade_proposal(&gas_schedule, true, "".to_owned().into_bytes())
            .unwrap()
            .pop()
            .unwrap();

    let gas_script_path = TempPath::new();
    let mut gas_script_path = gas_script_path.path().to_path_buf();
    gas_script_path.set_extension("move");
    fs::write(gas_script_path.as_path(), update_gas_script).unwrap();

    assert!(Command::new(aptos_cli.as_path())
        .current_dir(workspace_root())
        .args(&vec![
            "move",
            "run-script",
            "--script-path",
            gas_script_path.to_str().unwrap(),
            "--sender-account",
            "0xA550C18",
            "--url",
            url.as_str(),
            "--private-key",
            private_key.as_str(),
            "--assume-yes",
        ])
        .output()
        .unwrap()
        .status
        .success());
    *env.aptos_public_info().root_account().sequence_number_mut() += 1;

    let upgrade_scripts_folder = TempPath::new();
    upgrade_scripts_folder.create_as_dir().unwrap();

    let config = aptos_release_builder::ReleaseConfig {
        feature_flags: Some(Features {
            enabled: vec![
                FeatureFlag::CodeDependencyCheck,
                FeatureFlag::TreatFriendAsPrivate,
            ],
            disabled: vec![],
        }),
        ..Default::default()
    };

    config
        .generate_release_proposal_scripts(upgrade_scripts_folder.path())
        .unwrap();
    let mut scripts = fs::read_dir(upgrade_scripts_folder.path())
        .unwrap()
        .map(|res| res.unwrap().path())
        .collect::<Vec<_>>();

    scripts.sort();

    for path in scripts.iter() {
        assert!(Command::new(aptos_cli.as_path())
            .current_dir(workspace_root())
            .args(&vec![
                "move",
                "run-script",
                "--script-path",
                path.to_str().unwrap(),
                "--sender-account",
                "0xA550C18",
                "--url",
                url.as_str(),
                "--private-key",
                private_key.as_str(),
                "--assume-yes",
            ])
            .output()
            .unwrap()
            .status
            .success());

        *env.aptos_public_info().root_account().sequence_number_mut() += 1;
    }

    //TODO: Make sure gas schedule is indeed updated by the tool.

    // Test the module publishing workflow
    let base_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let base_path_v1 = base_dir.join("src/aptos/package_publish_modules_v1/");

    move_test_helpers::publish_package(&mut env.aptos_public_info(), base_path_v1)
        .await
        .unwrap();

    check_create_mint_transfer(&mut env).await;
}

#[tokio::test]
async fn test_upgrade_flow_multi_step() {
    // prebuild tools.
    let aptos_cli = workspace_builder::get_bin("aptos");

    let (mut env, mut cli, _) = SwarmBuilder::new_local(1)
        .with_init_config(Arc::new(|_, _, genesis_stake_amount| {
            // make sure we have quorum
            *genesis_stake_amount = 2000000000000000;
        }))
        .with_init_genesis_config(Arc::new(|genesis_config| {
            genesis_config.allow_new_validators = true;
            genesis_config.voting_duration_secs = 30;
            genesis_config.voting_power_increase_limit = 50;
            genesis_config.epoch_duration_secs = 4;
        }))
        .build_with_cli(2)
        .await;

    let url = env.aptos_public_info().url().to_string();
    let private_key = env
        .aptos_public_info()
        .root_account()
        .private_key()
        .to_encoded_string()
        .unwrap();

    let mut gas_parameters = AptosGasParameters::initial();
    gas_parameters.txn.max_transaction_size_in_bytes = GasQuantity::new(100_000_000);

    let gas_schedule = aptos_types::on_chain_config::GasScheduleV2 {
        feature_version: aptos_gas::LATEST_GAS_FEATURE_VERSION,
        entries: gas_parameters.to_on_chain_gas_schedule(),
    };

    let (_, update_gas_script) =
        generate_gas_upgrade_proposal(&gas_schedule, true, "".to_owned().into_bytes())
            .unwrap()
            .pop()
            .unwrap();

    let gas_script_path = TempPath::new();
    let mut gas_script_path = gas_script_path.path().to_path_buf();
    gas_script_path.set_extension("move");
    fs::write(gas_script_path.as_path(), update_gas_script).unwrap();

    assert!(Command::new(aptos_cli.as_path())
        .current_dir(workspace_root())
        .args(&vec![
            "move",
            "run-script",
            "--script-path",
            gas_script_path.to_str().unwrap(),
            "--sender-account",
            "0xA550C18",
            "--url",
            url.as_str(),
            "--private-key",
            private_key.as_str(),
            "--assume-yes",
        ])
        .output()
        .unwrap()
        .status
        .success());

    *env.aptos_public_info().root_account().sequence_number_mut() += 1;

    let upgrade_scripts_folder = TempPath::new();
    upgrade_scripts_folder.create_as_dir().unwrap();

    let config = aptos_release_builder::ReleaseConfig {
        feature_flags: Some(Features {
            enabled: vec![
                FeatureFlag::CodeDependencyCheck,
                FeatureFlag::TreatFriendAsPrivate,
            ],
            disabled: vec![],
        }),
        is_multi_step: true,
        ..Default::default()
    };

    config
        .generate_release_proposal_scripts(upgrade_scripts_folder.path())
        .unwrap();
    let mut scripts = fs::read_dir(upgrade_scripts_folder.path())
        .unwrap()
        .map(|res| res.unwrap().path())
        .collect::<Vec<_>>();

    scripts.sort();

    // Create a proposal and vote for it to pass.
    let mut i = 0;
    let mut validator_cli_index = 0;
    while i < 2 {
        let pool_address = cli.account_id(i);
        cli.fund_account(i, Some(1000000000000000)).await.unwrap();

        let mut keygen = KeyGen::from_os_rng();
        let (validator_cli_index, _) =
            init_validator_account(&mut cli, &mut keygen, Some(1000000000000000)).await;

        cli.initialize_stake_owner(
            i,
            1000000000000000,
            Some(validator_cli_index),
            Some(validator_cli_index),
        )
        .await
        .unwrap();

        cli.increase_lockup(i).await.unwrap();

        if i == 0 {
            let first_script_path = PathBuf::from(scripts.get(0).unwrap());
            cli.create_proposal(
                validator_cli_index,
                // placeholder url, will change later
                "https://gist.githubusercontent.com/movekevin/057fb145b40866eff8c22c91fb9da919/raw/bab85f0f7434f008a8781eec7fcadd1ac5a55481/gistfile1.txt",
                first_script_path,
                pool_address,
                true,
            ).await.unwrap();
        };
        cli.vote(validator_cli_index, 0, true, false, vec![pool_address])
            .await;
        i = i + 1;
    }

    // Sleep to pass voting_duration_secs
    thread::sleep(Duration::from_secs(30));

    let mut add_approved_execution_hash = true;
    for path in scripts.iter() {
        println!("path: {:?}", path.to_str().unwrap());
        let mut public_info = env.chain_info().into_aptos_public_info();
        let verify_proposal_response = cli
            .verify_proposal(0, path.to_str().unwrap())
            .await
            .unwrap();

        assert!(verify_proposal_response.verified);
        if add_approved_execution_hash {
            add_approved_script_hash_script()
        }

        let approved_execution_hash = public_info
            .get_approved_execution_hash_at_aptos_governance(0)
            .await;
        println!("{:?}", hex::encode(approved_execution_hash.clone()));
        println!("{:?}", verify_proposal_response.computed_hash);
        println!("{:?}", verify_proposal_response.onchain_hash);

        assert_eq!(
            verify_proposal_response.computed_hash,
            hex::encode(approved_execution_hash)
        );

        let args: Vec<ArgWithType> = vec![ArgWithType::u64(0)];
        cli.run_script_with_script_path(
            validator_cli_index,
            path.to_str().unwrap(),
            args,
            Vec::new(),
        )
        .await
        .unwrap();
    }

    // //TODO: Make sure gas schedule is indeed updated by the tool.
    // // Test the module publishing workflow
    // let base_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    // let base_path_v1 = base_dir.join("src/aptos/package_publish_modules_v1/");
    //
    // move_test_helpers::publish_package(&mut env.aptos_public_info(), base_path_v1)
    //     .await
    //     .unwrap();
    //
    // check_create_mint_transfer(&mut env).await;
}
