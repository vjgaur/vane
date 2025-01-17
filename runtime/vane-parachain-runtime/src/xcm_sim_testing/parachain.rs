use assets_common::matching::FromSiblingParachain;
use codec::{Decode, Encode};
use frame_support::{construct_runtime, match_types, parameter_types, traits::{EnsureOrigin, EnsureOriginWithArg, Everything, EverythingBut, Nothing, ContainsPair}, weights::{constants::WEIGHT_REF_TIME_PER_SECOND, Weight}};

use frame_system::EnsureRoot;
use sp_core::{ConstU32, H256, Get};
use sp_runtime::{
	testing::Header,
	traits::{Hash, IdentityLookup},
	AccountId32,
};
use sp_std::prelude::*;
use vane_xcm_transfer_system;


use pallet_xcm::XcmPassthrough;
use polkadot_core_primitives::BlockNumber as RelayBlockNumber;
use polkadot_parachain_primitives::primitives::{
	DmpMessageHandler, Id as ParaId, Sibling, XcmpMessageFormat, XcmpMessageHandler,
};
use staging_xcm::{latest::prelude::*, VersionedXcm};
use staging_xcm_builder::{Account32Hash, AccountId32Aliases, AliasForeignAccountId32, AllowUnpaidExecutionFrom, ConvertedConcreteId, CurrencyAdapter as XcmCurrencyAdapter, EnsureXcmOrigin, FixedRateOfFungible, FixedWeightBounds, IsConcrete, NativeAsset, NoChecking, NonFungiblesAdapter, ParentIsPreset, SiblingParachainConvertsVia, SignedAccountId32AsNative, SignedToAccountId32, SovereignSignedViaLocation};
use staging_xcm_executor::{
	traits::JustTry,
	Config, XcmExecutor,
};
use xcm_simulator::PhantomData;
use assets_common::foreign_creators::ForeignCreators;

use vane_primitive::{CurrencyId, MultiCurrencyAsset, MultiCurrencyConverter, VaneDerivedAssets, VaneForeignCreators};
use staging_xcm_executor::traits::MatchesFungible;
use sp_runtime::traits::{CheckedConversion, Convert};

// `EnsureOriginWithArg` impl for `CreateOrigin` which allows only XCM origins
// which are locations containing the class location.
use staging_xcm_executor::traits::ConvertLocation;
use crate::{ApprovalDeposit, ForeignAssetsAssetAccountDeposit, ForeignAssetsAssetDeposit, ForeignAssetsMetadataDepositBase, ForeignCreatorsSovereignAccountOf, MetadataDepositPerByte, StringLimit, weights, xcm_config};
use crate::xcm_sim_testing::Vane;

// pub struct ForeignCreators;
// impl EnsureOriginWithArg<RuntimeOrigin, MultiLocation> for ForeignCreators {
// 	type Success = AccountId;
//
// 	fn try_origin(
// 		o: RuntimeOrigin,
// 		a: &MultiLocation,
// 	) -> Result<Self::Success, RuntimeOrigin> {
// 		let origin_location = pallet_xcm::EnsureXcm::<Everything>::try_origin(o.clone())?;
// 		if !a.starts_with(&origin_location) {
// 			return Err(o)
// 		}
// 		SovereignAccountOf::convert_location(&origin_location).ok_or(o)
// 	}
//
// 	#[cfg(feature = "runtime-benchmarks")]
// 	fn try_successful_origin(a: &MultiLocation) -> Result<RuntimeOrigin, ()> {
// 		Ok(pallet_xcm::Origin::Xcm(a.clone()).into())
// 	}
// }

parameter_types! {
	pub const ReservedXcmpWeight: Weight = Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND.saturating_div(4), 0);
	pub const ReservedDmpWeight: Weight = Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND.saturating_div(4), 0);
}

parameter_types! {
	pub const KsmLocation: MultiLocation = MultiLocation::parent();
	pub const RelayNetwork: NetworkId = NetworkId::Kusama;
	pub UniversalLocation: InteriorMultiLocation = Parachain(MsgQueue::parachain_id().into()).into();
}

pub type LocationToAccountId = (
	ParentIsPreset<AccountId>,
	// SiblingParachainConvertsVia<Sibling, AccountId>,
	AccountId32Aliases<RelayNetwork, AccountId>,
	Account32Hash<(), AccountId>,
);

pub type XcmOriginToCallOrigin = (
	SovereignSignedViaLocation<LocationToAccountId, RuntimeOrigin>,
	SignedAccountId32AsNative<RelayNetwork, RuntimeOrigin>,
	//XcmPassthrough<RuntimeOrigin>,
);

parameter_types! {
	pub const UnitWeightCost: Weight = Weight::from_parts(1, 1);
	pub KsmPerSecondPerByte: (AssetId, u128, u128) = (Concrete(Parent.into()), 1, 1);
	pub const MaxInstructions: u32 = 100;
	pub const MaxAssetsIntoHolding: u32 = 64;
	pub ForeignPrefix: MultiLocation = (Parent,).into();
}


pub struct IsNativeConcrete<CurrencyId, CurrencyIdConvert>(sp_std::marker::PhantomData<(CurrencyId, CurrencyIdConvert)>);
impl<CurrencyId, CurrencyIdConvert, Amount> MatchesFungible<Amount> for IsNativeConcrete<CurrencyId, CurrencyIdConvert>
	where
		CurrencyIdConvert: Convert<MultiLocation, Option<CurrencyId>>,
		Amount: TryFrom<u128>,
{
	fn matches_fungible(a: &MultiAsset) -> Option<Amount> {
		if let (Fungible(ref amount), Concrete(ref location)) = (&a.fun, &a.id) {
			if CurrencyIdConvert::convert(*location).is_some() {
				return CheckedConversion::checked_from(*amount);
			}
		}
		None
	}
}


pub type LocalAssetTransactor =  vane_primitive::VaneMultiCurrencyAdapter<
	MultiCurrencyAsset<Runtime>,
	(), // handler for unknown assets
	IsNativeConcrete<CurrencyId, MultiCurrencyConverter<Runtime>>,
	AccountId,
	LocationToAccountId,
	CurrencyId,
	MultiCurrencyConverter<Runtime>,
	// HandlingFailedDeposits
>;

pub type SovereignAccountOf = (
	SiblingParachainConvertsVia<Sibling, AccountId>,
	AccountId32Aliases<RelayNetwork, AccountId>,
	ParentIsPreset<AccountId>,
);

pub type AccountId = AccountId32;
pub type Balance = u128;

parameter_types! {
	pub const BlockHashCount: u64 = 250;
}

impl frame_system::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type BaseCallFilter = Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = sp_runtime::traits::BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type BlockHashCount = BlockHashCount;
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
}

parameter_types! {
	pub ExistentialDeposit: Balance = 1;
	pub const MaxLocks: u32 = 50;
	pub const MaxReserves: u32 = 50;
}

impl pallet_balances::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type Balance = Balance;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type ReserveIdentifier = [u8; 8];
	type RuntimeHoldReason = RuntimeHoldReason;
	type FreezeIdentifier = ();
	type MaxLocks = MaxLocks;
	type MaxReserves = MaxReserves;
	type MaxHolds = ConstU32<0>;
	type MaxFreezes = ConstU32<0>;
}


pub type XcmRouter = super::ParachainXcmRouter<MsgQueue>;
pub type Barrier = AllowUnpaidExecutionFrom<Everything>;

match_types! {
	pub type SiblingPrefix: impl Contains<MultiLocation> = {
		MultiLocation { parents: 1, interior: X1(Parachain(_)) }
	};
	pub type ChildPrefix: impl Contains<MultiLocation> = {
		MultiLocation { parents: 0, interior: X1(Parachain(_)) }
	};
	pub type ParentPrefix: impl Contains<MultiLocation> = {
		MultiLocation { parents: 1, interior: Here }
	};
}




pub struct XcmConfig;
impl Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = XcmRouter;
	type AssetTransactor = LocalAssetTransactor;
	type OriginConverter = XcmOriginToCallOrigin;
	type IsReserve = vane_primitive::VaneDerivedAssets;
	type IsTeleporter = ();
	type Aliasers = AliasForeignAccountId32<ParentPrefix>; // test for both Parent and AssetHub Prefix
	type UniversalLocation = UniversalLocation;
	type Barrier = Barrier;
	type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
	type Trader = FixedRateOfFungible<KsmPerSecondPerByte, ()>;
	type ResponseHandler = ();
	type AssetTrap = ();
	type AssetLocker = PolkadotXcm;
	type AssetExchanger = ();
	type AssetClaims = ();
	type SubscriptionService = ();
	type PalletInstancesInfo = ();
	type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
	type FeeManager = ();
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Everything;
}


#[frame_support::pallet]
pub mod mock_msg_queue {
	use super::*;
	use frame_support::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		type XcmExecutor: ExecuteXcm<Self::RuntimeCall>;
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	#[pallet::getter(fn parachain_id)]
	pub(super) type ParachainId<T: Config> = StorageValue<_, ParaId, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn received_dmp)]
	/// A queue of received DMP messages
	pub(super) type ReceivedDmp<T: Config> = StorageValue<_, Vec<Xcm<T::RuntimeCall>>, ValueQuery>;

	impl<T: Config> Get<ParaId> for Pallet<T> {
		fn get() -> ParaId {
			Self::parachain_id()
		}
	}

	pub type MessageId = [u8; 32];

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		// XCMP
		/// Some XCM was executed OK.
		Success(Option<T::Hash>),
		/// Some XCM failed.
		Fail(Option<T::Hash>, XcmError),
		/// Bad XCM version used.
		BadVersion(Option<T::Hash>),
		/// Bad XCM format used.
		BadFormat(Option<T::Hash>),

		// DMP
		/// Downward message is invalid XCM.
		InvalidFormat(MessageId),
		/// Downward message is unsupported version of XCM.
		UnsupportedVersion(MessageId),
		/// Downward message executed with the given outcome.
		ExecutedDownward(MessageId, Outcome),
	}

	impl<T: Config> Pallet<T> {
		pub fn set_para_id(para_id: ParaId) {
			ParachainId::<T>::put(para_id);
		}

		fn handle_xcmp_message(
			sender: ParaId,
			_sent_at: RelayBlockNumber,
			xcm: VersionedXcm<T::RuntimeCall>,
			max_weight: Weight,
		) -> Result<Weight, XcmError> {
			let hash = Encode::using_encoded(&xcm, T::Hashing::hash);
			let message_hash = Encode::using_encoded(&xcm, sp_io::hashing::blake2_256);
			let (result, event) = match Xcm::<T::RuntimeCall>::try_from(xcm) {
				Ok(xcm) => {
					let location = (Parent, Parachain(sender.into()));
					match T::XcmExecutor::execute_xcm(location, xcm, message_hash, max_weight) {
						Outcome::Error(e) => (Err(e.clone()), Event::Fail(Some(hash), e)),
						Outcome::Complete(w) => (Ok(w), Event::Success(Some(hash))),
						// As far as the caller is concerned, this was dispatched without error, so
						// we just report the weight used.
						Outcome::Incomplete(w, e) => (Ok(w), Event::Fail(Some(hash), e)),
					}
				},
				Err(()) => (Err(XcmError::UnhandledXcmVersion), Event::BadVersion(Some(hash))),
			};
			Self::deposit_event(event);
			result
		}
	}

	impl<T: Config> XcmpMessageHandler for Pallet<T> {
		fn handle_xcmp_messages<'a, I: Iterator<Item = (ParaId, RelayBlockNumber, &'a [u8])>>(
			iter: I,
			max_weight: Weight,
		) -> Weight {
			for (sender, sent_at, data) in iter {
				let mut data_ref = data;
				let _ = XcmpMessageFormat::decode(&mut data_ref)
					.expect("Simulator encodes with versioned xcm format; qed");

				let mut remaining_fragments = &data_ref[..];
				while !remaining_fragments.is_empty() {
					if let Ok(xcm) =
						VersionedXcm::<T::RuntimeCall>::decode(&mut remaining_fragments)
					{
						let _ = Self::handle_xcmp_message(sender, sent_at, xcm, max_weight);
					} else {
						debug_assert!(false, "Invalid incoming XCMP message data");
					}
				}
			}
			max_weight
		}
	}

	impl<T: Config> DmpMessageHandler for Pallet<T> {
		fn handle_dmp_messages(
			iter: impl Iterator<Item = (RelayBlockNumber, Vec<u8>)>,
			limit: Weight,
		) -> Weight {
			for (_i, (_sent_at, data)) in iter.enumerate() {
				let id = sp_io::hashing::blake2_256(&data[..]);
				let maybe_versioned = VersionedXcm::<T::RuntimeCall>::decode(&mut &data[..]);
				match maybe_versioned {
					Err(_) => {
						Self::deposit_event(Event::InvalidFormat(id));
					},
					Ok(versioned) => match Xcm::try_from(versioned) {
						Err(()) => Self::deposit_event(Event::UnsupportedVersion(id)),
						Ok(x) => {
							let outcome = T::XcmExecutor::execute_xcm(Parent, x.clone(), id, limit);
							<ReceivedDmp<T>>::append(x);
							Self::deposit_event(Event::ExecutedDownward(id, outcome));
						},
					},
				}
			}
			limit
		}
	}
}





impl mock_msg_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
}

pub type LocalOriginToLocation = SignedToAccountId32<RuntimeOrigin, AccountId, RelayNetwork>;


pub struct TrustedLockerCase<T>(PhantomData<T>);
impl<T: Get<(MultiLocation, MultiAssetFilter)>> ContainsPair<MultiLocation, MultiAsset>
	for TrustedLockerCase<T>
{
	fn contains(origin: &MultiLocation, asset: &MultiAsset) -> bool {
		let (o, a) = T::get();
		a.matches(asset) && &o == origin
	}
}

parameter_types! {
	pub RelayTokenForRelay: (MultiLocation, MultiAssetFilter) = (Parent.into(), Wild(AllOf { id: Concrete(Parent.into()), fun: WildFungible }));
}

pub type TrustedLockers = TrustedLockerCase<RelayTokenForRelay>;

impl pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type CurrencyMatcher = ();
	type SendXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmRouter = XcmRouter;
	type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmExecuteFilter = Everything;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type XcmTeleportFilter = Nothing;
	type XcmReserveTransferFilter = Everything;
	type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
	type UniversalLocation = UniversalLocation;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
	type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
	type AdminOrigin = EnsureRoot<AccountId>;
	type TrustedLockers = TrustedLockers;
	type SovereignAccountOf = LocationToAccountId;
	type MaxLockers = ConstU32<8>;
	type MaxRemoteLockConsumers = ConstU32<0>;
	type RemoteLockConsumerIdentifier = ();
	type WeightInfo = pallet_xcm::TestWeightInfo;
}




impl vane_xcm_transfer_system::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
}
impl parachain_info::Config for Runtime {}

impl pallet_assets::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type RemoveItemsLimit = ConstU32<1000>;
	type AssetId = CurrencyId;
	type AssetIdParameter = CurrencyId;
	type Currency = Balances;
	type CreateOrigin = VaneForeignCreators<
		VaneDerivedAssets,
		ForeignCreatorsSovereignAccountOf,
		AccountId,
	>;
	type ForceOrigin = EnsureRoot<AccountId>;
	type AssetDeposit = ForeignAssetsAssetDeposit;
	type AssetAccountDeposit = ForeignAssetsAssetAccountDeposit;
	type MetadataDepositBase = ForeignAssetsMetadataDepositBase;
	type MetadataDepositPerByte = MetadataDepositPerByte;
	type ApprovalDeposit = ApprovalDeposit;
	type StringLimit = StringLimit;
	type Freezer = ();
	type Extra = ();
	type CallbackHandle = ();
	type WeightInfo = weights::vane_asset_weights::WeightInfo<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = xcm_config::XcmBenchmarkHelper;
}


type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Runtime>;
type Block = frame_system::mocking::MockBlock<Runtime>;

// Construct Runtime
construct_runtime!(
	pub enum Runtime
	{
		System: frame_system::{Pallet, Call, Storage, Config<T>, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
		MsgQueue: mock_msg_queue::{Pallet, Storage, Event<T>},
		PolkadotXcm: pallet_xcm::{Pallet, Call, Event<T>, Origin},
		VaneXcmTransfer: vane_xcm_transfer_system = 9,
		VaneAssets: pallet_assets = 10
	}
);


