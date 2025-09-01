# Luxor Swap

A Solana smart contract for Swapping SOL for LXR , staking SOL, distributing LXR token rewards, and managing buybacks, redemptions, and emergency controls.  
Built with [Anchor](https://github.com/coral-xyz/anchor).

---

## 📦 Features

- **Configurable Protocol State**  
  Initialize and update global parameters: admin, vaults, fees, limits, and feature flags.

- **SOL Staking → LXR Purchases**  
  Users can stake SOL to purchase LXR, with early-bird bonus logic and dynamic pricing.

- **Reward Accrual**  
  - Rewards in **SOL**: tracked via `reward_per_token_sol_stored`.  
  - Rewards in **LXR**: accrued via buybacks and distributed pro-rata with forfeiture rules.

- **Buyback Engine**  
  Uses SOL rewards accrued in the stake account to purchase LXR via Raydium CPMM, sending LXR to the reward vault and fees to treasury.

- **Redemption**  
  Users can redeem their share of accrued LXR rewards. If their current holdings are less than their base holdings, a portion is forfeited to treasury.

- **Emergency Controls**  
  Admin can withdraw vault funds, deactivate stake, or withdraw staked SOL in emergencies.

- **Event-Driven Indexing**  
  Emits structured events (`GlobalConfigInitialized`, `ConfigUpdated`, `LxrPurchased`, etc.) for off-chain monitoring.

---

## 🗂 Accounts

### GlobalConfig
Stores protocol-wide settings.

| Field | Type | Description |
|-------|------|-------------|
| `bump` | u8 | PDA bump |
| `admin` | Pubkey | Protocol admin |
| `lxr_treasury_vault` | Pubkey | Vault for treasury LXR |
| `lxr_reward_vault` | Pubkey | Vault for rewards LXR |
| `sol_treasury_vault` | Pubkey | Vault for SOL/WSOL fees |
| `stake_account` | Pubkey | Stake PDA (delegated to validator) |
| `vote_account` | Pubkey | Validator vote account |
| `stake_info` | Pubkey | Aggregated staking state |
| `bonus_rate` | u64 | Early-bird bonus rate |
| `max_stake_count_to_get_bonus` | u64 | Max stake count where bonus applies |
| `min_swap_amount` | u64 | Minimum LXR per purchase |
| `max_swap_amount` | u64 | Maximum LXR per purchase |
| `fee_treasury_rate` | u64 | Treasury fee rate |
| `purchase_enabled` | bool | Global purchase toggle |
| `redeem_enabled` | bool | Global redeem toggle |
| `initial_lxr_allocation_vault` | u64 | Initial allocation reference |

---

### StakeInfo
Aggregated staking and rewards state.

| Field | Type | Description |
|-------|------|-------------|
| `bump` | u8 | PDA bump |
| `total_staked_sol` | u64 | Total SOL staked |
| `total_stake_count` | u64 | Number of stakes |
| `total_sol_rewards_accrued` | u64 | Total SOL rewards accrued |
| `last_tracked_sol_balance` | u64 | Last observed SOL balance |
| `reward_per_token_sol_stored` | u128 | Global SOL reward index |
| `total_luxor_rewards_accrued` | u64 | Total LXR rewards accrued |
| `total_sol_used_for_buyback` | u64 | Total SOL used for buybacks |
| `last_update_timestamp` | u64 | Last update time |
| `last_buyback_timestamp` | u64 | Last buyback time |
| `reward_per_token_lxr_stored` | u128 | Global LXR reward index |
| `total_lxr_claimed` | u64 | Total LXR claimed by users |
| `total_lxr_forfeited` | u64 | Total LXR forfeited |

---

### UserStakeInfo
Tracks individual user’s staking and reward data.

| Field | Type | Description |
|-------|------|-------------|
| `bump` | u8 | PDA bump |
| `owner` | Pubkey | User pubkey |
| `total_staked_sol` | u64 | User’s staked SOL |
| `total_lxr_claimed` | u64 | Total LXR claimed |
| `total_lxr_forfeited` | u64 | Total LXR forfeited |
| `base_lxr_holdings` | u64 | Recorded baseline holdings |
| `lxr_reward_per_token_completed` | u128 | Reward index checkpoint |
| `lxr_rewards_pending` | u64 | Pending unclaimed rewards |

---

## ⚙️ Instructions

### `initialise_configs`
- Creates global config, vaults, and stake PDA.
- Sets admin, fee rates, feature flags.

### `update_config`
- Admin-only. Updates admin, swap limits, fee rates, purchase/redeem flags.

### `purchase`
- User stakes SOL to purchase LXR.
- Applies bonus logic, delegates stake, updates state.
- Transfers LXR to user, emits `LxrPurchased`.

### `manual_purchase`
- Admin-only. Records a purchase for a user with explicit amounts.
- Useful for backfills/adjustments.
- Emits `ManualLxrPurchased`.

### `buyback`
- Uses accrued SOL rewards to buy LXR on Raydium.
- Sends LXR to reward vault, fees to SOL treasury.
- Updates indices, emits `BuybackExecuted`.

### `redeem`
- User redeems accrued LXR rewards.
- If current holdings < baseline, applies forfeiture.
- Transfers claimable to user, forfeited to treasury.
- Emits `RewardsCollected`.

### `emergency_withdraw`
- Admin-only, modes:
  - `0`: Withdraw all LXR from treasury/reward vault
  - `1`: Withdraw all WSOL from SOL treasury vault
  - `2`: Deactivate stake PDA
  - `3`: Withdraw SOL from stake PDA

---

## 📡 Events

- **GlobalConfigInitialized** – emitted at protocol setup.  
- **ConfigUpdated** – parameters changed by admin.  
- **LxrPurchased** – user purchase executed.  
- **ManualLxrPurchased** – admin-recorded purchase.  
- **BuybackExecuted** – buyback executed with SOL rewards.  
- **RewardsCollected** – user claimed rewards (and forfeited portion).  

---