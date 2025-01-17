use std::sync::Once;
use xcm_emulator::*;
use polkadot_core_primitives::AccountPublic;
use sp_core::{sr25519, sr25519::Pair as PairType, Pair, Public};
use sp_core::crypto::Ss58AddressFormatRegistry;
use sp_runtime::MultiSigner;
use sp_runtime::traits::IdentifyAccount;
use crate::{AuraId,Balance};
use sp_core::crypto::Ss58Codec;
use sp_runtime::BuildStorage;
use staging_xcm_executor::traits::ConvertLocation;
use frame_support::traits::UnfilteredDispatchable;

const SAFE_XCM_VERSION: u32 =  crate::XCM_VERSION;

fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{}", seed), None)
		.expect("static values are valid; qed")
		.public()
}
fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
	where
		AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
	AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

pub fn get_collator_keys_from_seed(seed: &str) -> AuraId {
	get_from_seed::<AuraId>(seed)
}

pub fn template_session_keys(keys: AuraId) -> crate::SessionKeys {
	crate::SessionKeys { aura: keys }
}

#[derive(Encode,Decode)]
pub struct RococoId(u32);

fn calculate_sovereign_account<Pair>(
	para_id: u32,
) -> Result<String, Box<dyn std::error::Error>>
	where
		Pair: sp_core::Pair,
		Pair::Public: Into<MultiSigner>,
{
	// Scale encoded para_id
	let id = RococoId(para_id);
	let encoded_id = hex::encode(id.encode());

	// Prefix para or sibl
	let prefix = hex::encode("para");

	// Join both strings and the 0x at the beginning
	let encoded_key = "0x".to_owned() + &prefix + &encoded_id;

	// Fill the rest with 0s
	let public_str = format!("{:0<width$}", encoded_key, width = 64 + 2);

	// Convert hex public key to ss58 address
	let public = array_bytes::hex2bytes(&public_str).expect("Failed to convert hex to bytes");
	let public_key = Pair::Public::try_from(&public)
		.map_err(|_| "Failed to construct public key from given hex")?;

	Ok(public_key.to_ss58check_with_version(Ss58AddressFormatRegistry::SubstrateAccount.into()))
}



pub mod accounts {
	use sp_core::sr25519;
	use crate::AuraId;
	use super::*;
	pub const ALICE: &str = "Alice";
	pub const BOB: &str = "Bob";
	pub const CHARLIE: &str = "Charlie";
	pub const DAVE: &str = "Dave";
	pub const EVE: &str = "Eve";


	pub fn init_balances() -> Vec<AccountId> {
		vec![
			get_account_id_from_seed::<sr25519::Public>(ALICE),
			get_account_id_from_seed::<sr25519::Public>(BOB),
			get_account_id_from_seed::<sr25519::Public>(CHARLIE),
			get_account_id_from_seed::<sr25519::Public>(DAVE),
			get_account_id_from_seed::<sr25519::Public>(EVE),

		]
	}

	pub fn invulnerables() -> Vec<(AccountId, AuraId)> {
		vec![
			(
				get_account_id_from_seed::<sr25519::Public>("Alice"),
				get_collator_keys_from_seed("Alice"),
			),
			(
				get_account_id_from_seed::<sr25519::Public>("Bob"),
				get_collator_keys_from_seed("Bob"),
			),
		]
	}

	pub fn sudo_key() -> AccountId {
		get_account_id_from_seed::<sr25519::Public>(ALICE)
	}
}

pub use vane_parachain::*;

pub mod vane_parachain {
	use super::*;
	use crate::xcm_eml_testing::Storage;	use sp_core::crypto::Ss58Codec;
	use vane_primitive::CurrencyId;
	use vane_primitive::CurrencyId::DOT;
	use crate::{EXISTENTIAL_DEPOSIT,Balance};
	use crate::xcm_eml_testing::accounts::{ALICE, invulnerables, sudo_key};


	pub const PARA_ID: u32 = 2000;
	pub const ED: Balance = EXISTENTIAL_DEPOSIT;
	pub fn genesis() -> Storage {

		let v_dot = "vDOT".as_bytes().to_vec();
		let _v_usdt = "vUSDT".as_bytes().to_vec();
		let _v_usdc = "vUSDC".as_bytes().to_vec();

		// Calculate parachain Soverign account id
		let sovererign_acount = calculate_sovereign_account::<PairType>(PARA_ID.into()).unwrap();
		let para_account = sp_runtime::AccountId32::from_ss58check(&sovererign_acount).unwrap();
        let alice = get_account_id_from_seed::<sr25519::Public>(ALICE);

		// ---******* GENESIS CONFIG ********---//

		let genesis_config = crate::RuntimeGenesisConfig {

			system: crate::SystemConfig {
				code: crate::WASM_BINARY
					.expect("WASM binary was not build, please build it!")
					.to_vec(),
				..Default::default()
			},

			balances: crate::BalancesConfig {
				balances: accounts::init_balances()
					.iter()
					.cloned()
					.map(|k| (k, ED * 4096))
					.collect(),
			},

			vane_assets: crate::VaneAssetsConfig {

				metadata: vec![(DOT,v_dot.clone(), v_dot,10)],

				assets: vec![(DOT,para_account.clone(),true,1)],

				accounts: vec![(DOT,para_account.clone(),0)]

			},

			vane_xcm_transfer: crate::VaneXcmTransferConfig {
				para_account: Some(para_account)
			},

			parachain_info: crate::ParachainInfoConfig {
				parachain_id: PARA_ID.into(),
				..Default::default()
			},

			collator_selection: crate::CollatorSelectionConfig {
				invulnerables: invulnerables().iter().cloned().map(|(acc, _)| acc).collect(),
				candidacy_bond: EXISTENTIAL_DEPOSIT * 16,
				..Default::default()
			},

			session: crate::SessionConfig {
				keys: invulnerables()
					.into_iter()
					.map(|(acc, aura)| {
						(
							acc.clone(),                 // account id
							acc,                         // validator id
							template_session_keys(aura), // session keys
						)
					})
					.collect(),
			},

			polkadot_xcm: crate::PolkadotXcmConfig {
				safe_xcm_version: Some(SAFE_XCM_VERSION),
				..Default::default()
			},

			sudo: crate::SudoConfig { key: Some(alice) },
			..Default::default()
		};

		genesis_config.build_storage().unwrap()

	}

}

// decl_test_relay_chains! {
// 	#[api_version(5)]
// 	pub struct PolkadotMain {
// 		genesis = polkadot::genesis(),
// 		on_init = (),
// 		runtime = {
// 			Runtime: polkadot_runtime::Runtime,
// 			RuntimeOrigin: polkadot_runtime::RuntimeOrigin,
// 			RuntimeCall: polkadot_runtime::RuntimeCall,
// 			RuntimeEvent: polkadot_runtime::RuntimeEvent,
// 			MessageQueue: polkadot_runtime::MessageQueue,
// 			XcmConfig: polkadot_runtime::xcm_config::XcmConfig,
// 			SovereignAccountOf: polkadot_runtime::xcm_config::SovereignAccountOf,
// 			System: polkadot_runtime::System,
// 			Balances: polkadot_runtime::Balances,
// 		},
//
// 		pallets = {
// 			XcmPallet: polkadot_runtime::XcmPallet,
// 		}
// 	},
// 	#[api_version(5)]
// 	pub struct Rococo {
// 		genesis = rococo::genesis(),
// 		on_init = (),
// 		runtime = {
// 			Runtime: rococo_runtime::Runtime,
// 			RuntimeOrigin: rococo_runtime::RuntimeOrigin,
// 			RuntimeCall: rococo_runtime::RuntimeCall,
// 			RuntimeEvent: rococo_runtime::RuntimeEvent,
// 			MessageQueue: rococo_runtime::MessageQueue,
// 			XcmConfig: rococo_runtime::xcm_config::XcmConfig,
// 			SovereignAccountOf: rococo_runtime::xcm_config::LocationConverter, //TODO: rename to SovereignAccountOf,
// 			System: rococo_runtime::System,
// 			Balances: rococo_runtime::Balances,
// 		},
// 		pallets_extra = {
// 			XcmPallet: rococo_runtime::XcmPallet,
// 			Sudo: rococo_runtime::Sudo,
// 		}
// 	}
// }
//
// decl_test_parachains!(
// 		pub struct VaneParachain {
// 		genesis = vane_parachain::genesis(),
// 		on_init = (),
// 		runtime = {
// 			Runtime: crate::Runtime,
// 			RuntimeOrigin: crate::RuntimeOrigin,
// 			RuntimeCall: crate::RuntimeCall,
// 			RuntimeEvent: crate::RuntimeEvent,
// 			XcmpMessageHandler: crate::XcmpQueue,
// 			DmpMessageHandler: crate::DmpQueue,
// 			LocationToAccountId: crate::xcm_config::LocationToAccountId,
// 			System: crate::System,
// 			Balances: crate::Balances,
// 			ParachainSystem: crate::ParachainSystem,
// 			ParachainInfo: crate::ParachainInfo,
// 		},
// 		pallets = {
// 			PolkadotXcm: crate::PolkadotXcm,
// 			VaneAssets: crate::VaneAssets,
// 			VaneXcm: crate::VaneXcm,
//
// 		}
// 	},
// 		pub struct VaneRococo {
// 		genesis = vane_parachain::genesis(),
// 		on_init = (),
// 		runtime = {
// 			Runtime: crate::Runtime,
// 			RuntimeOrigin: crate::RuntimeOrigin,
// 			RuntimeCall: crate::RuntimeCall,
// 			RuntimeEvent: crate::RuntimeEvent,
// 			XcmpMessageHandler: crate::XcmpQueue,
// 			DmpMessageHandler: crate::DmpQueue,
// 			LocationToAccountId: crate::xcm_config::LocationToAccountId,
// 			System: crate::System,
// 			Balances: crate::Balances,
// 			ParachainSystem: crate::ParachainSystem,
// 			ParachainInfo: crate::ParachainInfo,
// 		},
// 		pallets = {
// 			PolkadotXcm: crate::PolkadotXcm,
// 			VaneAssets: crate::VaneAssets,
// 			VaneXcm: crate::VaneXcm,
//
// 		}
// 	},
// 	pub struct AssetHubPolkadot {
// 		genesis = asset_hub_polkadot::genesis(),
// 		on_init = (),
// 		runtime = {
// 			Runtime: asset_hub_polkadot_runtime::Runtime,
// 			RuntimeOrigin: asset_hub_polkadot_runtime::RuntimeOrigin,
// 			RuntimeCall: asset_hub_polkadot_runtime::RuntimeCall,
// 			RuntimeEvent: asset_hub_polkadot_runtime::RuntimeEvent,
// 			XcmpMessageHandler: asset_hub_polkadot_runtime::XcmpQueue,
// 			DmpMessageHandler: asset_hub_polkadot_runtime::DmpQueue,
// 			LocationToAccountId: asset_hub_polkadot_runtime::xcm_config::LocationToAccountId,
// 			System: asset_hub_polkadot_runtime::System,
// 			Balances: asset_hub_polkadot_runtime::Balances,
// 			ParachainSystem: asset_hub_polkadot_runtime::ParachainSystem,
// 			ParachainInfo: asset_hub_polkadot_runtime::ParachainInfo,
// 		},
// 		pallets = {
// 			PolkadotXcm: asset_hub_polkadot_runtime::PolkadotXcm,
// 			Assets: asset_hub_polkadot_runtime::Assets,
// 		}
// 	},
// 	pub struct AssetHubRococo {
// 		genesis = asset_hub_polkadot::genesis(),
// 		on_init = (),
// 		runtime = {
// 			Runtime: asset_hub_polkadot_runtime::Runtime,
// 			RuntimeOrigin: asset_hub_polkadot_runtime::RuntimeOrigin,
// 			RuntimeCall: asset_hub_polkadot_runtime::RuntimeCall,
// 			RuntimeEvent: asset_hub_polkadot_runtime::RuntimeEvent,
// 			XcmpMessageHandler: asset_hub_polkadot_runtime::XcmpQueue,
// 			DmpMessageHandler: asset_hub_polkadot_runtime::DmpQueue,
// 			LocationToAccountId: asset_hub_polkadot_runtime::xcm_config::LocationToAccountId,
// 			System: asset_hub_polkadot_runtime::System,
// 			Balances: asset_hub_polkadot_runtime::Balances,
// 			ParachainSystem: asset_hub_polkadot_runtime::ParachainSystem,
// 			ParachainInfo: asset_hub_polkadot_runtime::ParachainInfo,
// 		},
// 		pallets = {
// 			PolkadotXcm: asset_hub_polkadot_runtime::PolkadotXcm,
// 			Assets: asset_hub_polkadot_runtime::Assets,
// 		}
// 	}
// );
//
// decl_test_networks!(
// 	// Polkadot
// 		pub struct PolkadotNet {
// 		relay_chain = PolkadotMain,
// 		parachains = vec![
// 			AssetHubPolkadot,
// 			VaneParachain,
// 		],
// 		bridge = ()
// 	},
//
// 	// Rococo
// 	pub struct RococoNet {
// 		relay_chain = Rococo,
// 		parachains = vec![
// 			AssetHubRococo,
// 			VaneRococo,
// 		],
// 		bridge = ()
// 	}
// );
//
//
// static INIT: Once = Once::new();
// pub fn init_tracing() {
// 	INIT.call_once(|| {
// 		// Add test tracing (from sp_tracing::init_for_tests()) but filtering for xcm logs only
// 		let _ = tracing_subscriber::fmt()
// 			//.with_max_level(tracing::Level::TRACE)
// 			//.with_env_filter("xcm=trace,system::events=trace") // Comment out this line to see all traces
// 			.with_test_writer()
// 			.init();
// 	});
// }
//
//
// // Tests
//
// mod tests {
// 	use super::*;
// 	use frame_support::assert_ok;
// 	use frame_support::dispatch::RawOrigin;
// 	use hex_literal::hex;
// 	use staging_xcm::latest::OriginKind::SovereignAccount;
// 	use xcm_emulator::{Parachain, TestExt};
// 	use crate::xcm_eml_testing::{VaneParachain,PolkadotMain};
// 	use crate::xcm_eml_testing::accounts::{ALICE,BOB,CHARLIE};
// 	use staging_xcm::VersionedXcm;
// 	use sp_tracing;
// 	use vane_primitive::CurrencyId::DOT;
//
// 	#[test]
// 	fn relay_chain_n_vane_remote_execution_works() {
//
//
// 		// Alice --> RC                                           RC
// 		//           -  (Reserve transfer)                         ^
// 		//           ˯                                             -
// 		//      Reserve Chain                                 Reserve Chain
// 		//           -  (Deposit Equivalent)                       ^
// 		//           ˯                                             -
// 		//         Vane  --------> MultiSig(Alice,Bob) --------> VaneXcm
// 		//           -        									   ^
// 		//           - ----------> Confirmation                    -
// 		//                          -                              -
// 		//                          --->Ms(A,B)--->Bob -------------
// 	}
// 	#[test]
// 	fn native_reserve_transfer_reserve_works() {
//
// 	}
// }
