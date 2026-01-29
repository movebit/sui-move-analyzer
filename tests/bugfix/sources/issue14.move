module typus_stake_pool::stake_pool {
    use std::type_name::{Self, TypeName};
    use std::string::{Self, String};
    use typus::keyed_big_vector::{Self, KeyedBigVector};
    use sui::clock::{Self, Clock};
    use sui::dynamic_field;
    use sui::dynamic_object_field;
    use sui::vec_map::{Self, VecMap};

    // ======== Keys ========
    const K_LP_USER_SHARES: vector<u8> = b"lp_user_shares";

    /// A registry for all stake pools.
    public struct StakePoolRegistry has key {
        id: UID,
        /// The number of pools in the registry.
        num_pool: u64,
    }

    /// A struct that represents a stake pool.
    public struct StakePool has key, store {
        id: UID,
        /// Information about the stake pool.
        pool_info: StakePoolInfo,
        /// Configuration for the stake pool.
        config: StakePoolConfig,
        /// A vector of the incentives in the stake pool.
        incentives: vector<Incentive>,
        /// Padding for future use.
        u64_padding: vector<u64>,
    }

    /// A struct that holds information about an incentive.
    public struct Incentive has copy, drop, store {
        /// The type name of the incentive token.
        token_type: TypeName,
        /// The configuration for the incentive.
        config: IncentiveConfig,
        /// Information about the incentive.
        info: IncentiveInfo
    }

    /// Information about a stake pool.
    public struct StakePoolInfo has copy, drop, store {
        /// The type name of the stake token.
        stake_token: TypeName,
        /// The index of the pool.
        index: u64,
        /// The next user share ID.
        next_user_share_id: u64,
        /// The total number of shares in the pool.
        total_share: u64, // = total staked and has not been unsubscribed
        /// Whether the pool is active.
        active: bool,
        /// tlp price (decimal 4)
        new_tlp_price: u64,
        /// number of depositor
        depositors_count: u64,
        /// Padding for future use.
        u64_padding: vector<u64>,
    }

    /// Configuration for a stake pool.
    public struct StakePoolConfig has copy, drop, store {
        /// The unlock countdown in milliseconds.
        unlock_countdown_ts_ms: u64,
        /// for exp calculation
        usd_per_exp: u64,
        /// Padding for future use.
        u64_padding: vector<u64>,
    }

    /// Configuration for an incentive.
    public struct IncentiveConfig has copy, drop, store {
        /// The amount of incentive per period.
        period_incentive_amount: u64,
        /// The incentive interval in milliseconds.
        incentive_interval_ts_ms: u64,
        /// Padding for future use.
        u64_padding: vector<u64>,
    }

    /// Information about an incentive.
    public struct IncentiveInfo has copy, drop, store {
        /// Whether the incentive is active.
        active: bool,
        /// The timestamp of the last allocation.
        last_allocate_ts_ms: u64, // record allocate ts ms for each I_TOKEN
        /// The price index for accumulating incentive.
        incentive_price_index: u64, // price index for accumulating incentive
        /// The unallocated amount of incentive.
        unallocated_amount: u64,
        /// Padding for future use.
        u64_padding: vector<u64>,
    }

    /// A struct that represents a user's share in a stake pool.
    public struct LpUserShare has store {
        /// The address of the user.
        user: address,
        /// The ID of the user's share.
        user_share_id: u64,
        /// The timestamp when the user staked.
        stake_ts_ms: u64,
        /// The total number of shares.
        total_shares: u64,
        /// The number of active shares.
        active_shares: u64,
        /// A vector of deactivating shares.
        deactivating_shares: vector<DeactivatingShares>,
        /// The last incentive price index.
        last_incentive_price_index: VecMap<TypeName, u64>,
        /// The last snapshot ts for exp.
        snapshot_ts_ms: u64,
        /// old tlp price  for exp with decimal 4
        tlp_price: u64,
        /// accumulated harvested amount
        harvested_amount: u64,
        /// Padding for future use.
        u64_padding: vector<u64>,
    }

    /// A struct for deactivating shares.
    public struct DeactivatingShares has store {
        /// The number of shares.
        shares: u64,
        /// The timestamp when the user unsubscribed.
        unsubscribed_ts_ms: u64,
        /// The timestamp when the shares can be unlocked.
        unlocked_ts_ms: u64,
        /// The unsubscribed incentive price index.
        unsubscribed_incentive_price_index: VecMap<TypeName, u64>, // the share can only receive incentive until this index
        /// Padding for future use.
        u64_padding: vector<u64>,
    }

    /// An event that is emitted when a new stake pool is created.
    public struct NewStakePoolEvent has copy, drop {
        sender: address,
        stake_pool_info: StakePoolInfo,
        stake_pool_config: StakePoolConfig,
        u64_padding: vector<u64>
    }

    public struct AutoCompoundEvent has copy, drop {
        sender: address,
        index: u64,
        incentive_token: TypeName,
        incentive_price_index: u64,
        total_amount: u64,
        compound_users: u64,
        total_users: u64,
        u64_padding: vector<u64>
    }
    fun get_mut_stake_pool(
        id: &mut UID,
        index: u64,
    ): &mut StakePool {
        dynamic_object_field::borrow_mut<u64, StakePool>(id, index)
    }

    fun calculate_incentive(
        current_incentive_index: u64,
        incentive_token: &TypeName,
        lp_user_share: &LpUserShare,
    ): (u64, u64) {
        (0, 0)
    }

    fun update_last_incentive_price_index(lp_user_share: &mut LpUserShare, incentive_token: TypeName, current_incentive_index: u64) {
        if (vec_map::contains(&lp_user_share.last_incentive_price_index, &incentive_token)) {
            let last_incentive_price_index = vec_map::get_mut(&mut lp_user_share.last_incentive_price_index, &incentive_token);
            *last_incentive_price_index = current_incentive_index;
        } else {
            vec_map::insert(&mut lp_user_share.last_incentive_price_index, incentive_token, current_incentive_index);
        };
    }
    
    fun log_harvested_amount(user_share: &mut LpUserShare, incentive_value: u64) {
        user_share.harvested_amount = user_share.harvested_amount + incentive_value;
    }

    /// [Authorized Function]
    entry fun auto_compound<I_TOKEN>(
        version: &Version,
        registry: &mut StakePoolRegistry,
        index: u64,
        clock: &Clock,
        ctx: & TxContext
    ) {
        let stake_pool = get_mut_stake_pool(&mut registry.id, index);
        let incentive_token = type_name::with_defining_ids<I_TOKEN>();
        let current_incentive_index = 1;

        let user_shares = dynamic_field::borrow_mut<String, KeyedBigVector>(&mut stake_pool.id, string::utf8(K_LP_USER_SHARES));

        let mut total_incentive_value = 0;
        let mut compound_users = 0;
        // bug1: cannot goto do_mut which is a macro function
        // bug2: cannot use lsp feature when code is in macro body
        user_shares.do_mut!(|_user: address, lp_user_share: &mut LpUserShare| {
            let (incentive_value, _) = calculate_incentive(current_incentive_index, &incentive_token, lp_user_share);
            lp_user_share.update_last_incentive_price_index(incentive_token, current_incentive_index);
            // accumulate incentive_value
            lp_user_share.log_harvested_amount(incentive_value);

            // handle user share incentive_value
            total_incentive_value = total_incentive_value + incentive_value;
            lp_user_share.total_shares = lp_user_share.total_shares + incentive_value;
            lp_user_share.active_shares = lp_user_share.active_shares + incentive_value;

            compound_users = compound_users + 1;
        });
    }
}