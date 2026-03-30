use super::constants::*;
use crate::{num_words, tri, SStoreResult, SelfDestructResult, StateLoad};
use context_interface::{
    journaled_state::AccountLoad, transaction::AccessListItemTr as _, Transaction, TransactionType,
};
use primitives::{eip7702, hardfork::SpecId, U256};

/// `SSTORE` refunds are disabled under Monad's page-based storage pricing.
#[inline]
pub const fn sstore_refund(_spec_id: SpecId, _vals: &SStoreResult) -> i64 {
    0
}

/// `CREATE2` opcode cost calculation.
#[inline]
pub const fn create2_cost(len: usize) -> Option<u64> {
    CREATE.checked_add(tri!(cost_per_word(len, KECCAK256WORD)))
}

#[inline]
const fn log2floor(value: U256) -> u64 {
    let mut l: u64 = 256;
    let mut i = 3;
    loop {
        if value.as_limbs()[i] == 0u64 {
            l -= 64;
        } else {
            l -= value.as_limbs()[i].leading_zeros() as u64;
            if l == 0 {
                return l;
            } else {
                return l - 1;
            }
        }
        if i == 0 {
            break;
        }
        i -= 1;
    }
    l
}

/// `EXP` opcode cost calculation.
#[inline]
pub fn exp_cost(spec_id: SpecId, power: U256) -> Option<u64> {
    if power.is_zero() {
        Some(EXP)
    } else {
        // EIP-160: EXP cost increase
        let gas_byte = U256::from(if spec_id.is_enabled_in(SpecId::SPURIOUS_DRAGON) {
            50
        } else {
            10
        });
        let gas = U256::from(EXP)
            .checked_add(gas_byte.checked_mul(U256::from(log2floor(power) / 8 + 1))?)?;

        u64::try_from(gas).ok()
    }
}

/// `*COPY` opcodes cost calculation.
#[inline]
pub const fn copy_cost_verylow(len: usize) -> Option<u64> {
    copy_cost(VERYLOW, len)
}

/// `EXTCODECOPY` opcode cost calculation.
#[inline]
pub const fn extcodecopy_cost(spec_id: SpecId, len: usize, is_cold: bool) -> Option<u64> {
    let base_gas = if spec_id.is_enabled_in(SpecId::BERLIN) {
        warm_cold_cost(is_cold)
    } else if spec_id.is_enabled_in(SpecId::TANGERINE) {
        700
    } else {
        20
    };
    copy_cost(base_gas, len)
}

#[inline]
/// Calculates the gas cost for copy operations based on data length.
pub const fn copy_cost(base_cost: u64, len: usize) -> Option<u64> {
    base_cost.checked_add(tri!(cost_per_word(len, COPY)))
}

/// `LOG` opcode cost calculation.
#[inline]
pub const fn log_cost(n: u8, len: u64) -> Option<u64> {
    tri!(LOG.checked_add(tri!(LOGDATA.checked_mul(len)))).checked_add(LOGTOPIC * n as u64)
}

/// `KECCAK256` opcode cost calculation.
#[inline]
pub const fn keccak256_cost(len: usize) -> Option<u64> {
    KECCAK256.checked_add(tri!(cost_per_word(len, KECCAK256WORD)))
}

/// Calculate the cost of buffer per word.
#[inline]
pub const fn cost_per_word(len: usize, multiple: u64) -> Option<u64> {
    multiple.checked_mul(num_words(len) as u64)
}

/// EIP-3860: Limit and meter initcode
///
/// Apply extra gas cost of 2 for every 32-byte chunk of initcode.
///
/// This cannot overflow as the initcode length is assumed to be checked.
#[inline]
pub const fn initcode_cost(len: usize) -> u64 {
    let Some(cost) = cost_per_word(len, INITCODE_WORD_COST) else {
        panic!("initcode cost overflow")
    };
    cost
}

/// `SLOAD` opcode cost calculation.
#[inline]
pub const fn sload_cost(_spec_id: SpecId, is_cold: bool) -> u64 {
    if is_cold {
        COLD_SLOAD_COST
    } else {
        BASE_SLOAD_COST
    }
}

/// Static gas cost for sstore.
#[inline]
pub const fn sstore_cost_static(_spec_id: SpecId) -> u64 {
    0
}

/// Dynamic gas cost for sstore.
#[inline]
pub const fn sstore_cost_dynamic(spec_id: SpecId, vals: &SStoreResult, is_cold: bool) -> u64 {
    sstore_cost(spec_id, vals, is_cold)
}

/// Static gas cost for sstore.
#[inline]
pub const fn static_sstore_cost(_spec_id: SpecId) -> u64 {
    0
}

/// Dynamic gas cost for sstore.
#[inline]
pub const fn dyn_sstore_cost(spec_id: SpecId, vals: &SStoreResult, is_cold: bool) -> u64 {
    sstore_cost(spec_id, vals, is_cold)
}

/// `SSTORE` opcode cost calculation.
#[inline]
pub const fn sstore_cost(_spec_id: SpecId, vals: &SStoreResult, is_cold: bool) -> u64 {
    let mut gas_cost = if vals.is_original_zero() && vals.is_present_zero() && !vals.is_new_zero() {
        BASE_SSTORE_COST
    } else if !vals.is_original_zero() && !vals.is_present_zero() && vals.is_new_zero() {
        BASE_SSTORE_COST
    } else if vals.is_original_zero() && !vals.is_present_zero() && vals.is_new_zero() {
        0
    } else if !vals.is_original_zero() && vals.is_present_zero() && !vals.is_new_zero() {
        BASE_SSTORE_COST
    } else {
        BASE_SSTORE_COST
    };

    if is_cold {
        gas_cost += COLD_SLOAD_COST;
    } else {
        gas_cost += BASE_SLOAD_COST;
    }

    if vals.page_write_charged {
        gas_cost += PAGE_WRITE_COST;
    }

    if vals.new_slot_cost_charged {
        gas_cost += NEW_SLOT_COST;
    }

    gas_cost
}

/// Static gas cost for selfdestruct.
#[inline]
pub const fn static_selfdestruct_cost(spec_id: SpecId) -> u64 {
    // EIP-150: Gas cost changes for IO-heavy operations
    if spec_id.is_enabled_in(SpecId::TANGERINE) {
        5000
    } else {
        0
    }
}

/// `SELFDESTRUCT` opcode cost calculation.
#[inline]
pub const fn dyn_selfdestruct_cost(spec_id: SpecId, res: &StateLoad<SelfDestructResult>) -> u64 {
    let is_tangerine = spec_id.is_enabled_in(SpecId::TANGERINE);
    let mut gas = 0;

    // EIP-161: State trie clearing (invariant-preserving alternative)
    let should_charge_topup = if spec_id.is_enabled_in(SpecId::SPURIOUS_DRAGON) {
        res.data.had_value && !res.data.target_exists
    } else {
        !res.data.target_exists
    };

    // EIP-150: Gas cost changes for IO-heavy operations
    if is_tangerine && should_charge_topup {
        gas += NEWACCOUNT
    }

    if res.is_cold {
        gas += selfdestruct_cold_beneficiary_cost(spec_id);
    }

    gas
}

/// EIP-2929: Gas cost increases for state access opcodes
#[inline]
pub const fn selfdestruct_cold_beneficiary_cost(spec_id: SpecId) -> u64 {
    if spec_id.is_enabled_in(SpecId::BERLIN) {
        COLD_ACCOUNT_ACCESS_COST
    } else {
        0
    }
}

/// `SELFDESTRUCT` opcode cost calculation.
#[inline]
pub const fn selfdestruct_cost(spec_id: SpecId, res: StateLoad<SelfDestructResult>) -> u64 {
    static_selfdestruct_cost(spec_id) + dyn_selfdestruct_cost(spec_id, &res)
}

/// Calculate static gas for the call
///
/// Gas depends on:
/// * Spec. For berlin hardfork only warm gas [`WARM_STORAGE_READ_COST`] is calculated.
/// * If there is transfer value. additional gas of [`CALLVALUE`] is added.
#[inline]
pub fn calc_call_static_gas(spec_id: SpecId, has_transfer: bool) -> u64 {
    // Account access.
    let mut gas = if spec_id.is_enabled_in(SpecId::BERLIN) {
        WARM_STORAGE_READ_COST
    } else if spec_id.is_enabled_in(SpecId::TANGERINE) {
        // EIP-150: Gas cost changes for IO-heavy operations
        700
    } else {
        40
    };

    // Transfer value cost
    if has_transfer {
        gas += CALLVALUE;
    }

    gas
}

/// Berlin warm and cold storage access cost for account access.
#[inline]
pub const fn warm_cold_cost(is_cold: bool) -> u64 {
    if is_cold {
        COLD_ACCOUNT_ACCESS_COST
    } else {
        WARM_STORAGE_READ_COST
    }
}

/// Berlin warm and cold storage access cost for account access.
///
/// If delegation is Some, add additional cost for delegation account load.
#[inline]
pub const fn warm_cold_cost_with_delegation(load: StateLoad<AccountLoad>) -> u64 {
    let mut gas = warm_cold_cost(load.is_cold);
    if let Some(is_cold) = load.data.is_delegate_account_cold {
        gas += warm_cold_cost(is_cold);
    }
    gas
}

/// Memory expansion cost calculation for a given number of words.
#[inline]
pub const fn memory_gas(num_words: usize) -> u64 {
    (num_words as u64) / 2
}

/// Init and floor gas from transaction
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct InitialAndFloorGas {
    /// Initial gas for transaction.
    pub initial_gas: u64,
    /// If transaction is a Call and Prague is enabled
    /// floor_gas is at least amount of gas that is going to be spent.
    pub floor_gas: u64,
}

impl InitialAndFloorGas {
    /// Create a new InitialAndFloorGas instance.
    #[inline]
    pub const fn new(initial_gas: u64, floor_gas: u64) -> Self {
        Self {
            initial_gas,
            floor_gas,
        }
    }
}

/// Initial gas that is deducted for transaction to be included.
/// Initial gas contains initial stipend gas, gas for access list and input data.
///
/// # Returns
///
/// - Intrinsic gas
/// - Number of tokens in calldata
pub fn calculate_initial_tx_gas(
    spec_id: SpecId,
    input: &[u8],
    is_create: bool,
    access_list_accounts: u64,
    access_list_storages: u64,
    authorization_list_num: u64,
) -> InitialAndFloorGas {
    let mut gas = InitialAndFloorGas::default();

    // Initdate stipend
    let tokens_in_calldata = get_tokens_in_calldata(input, spec_id.is_enabled_in(SpecId::ISTANBUL));

    gas.initial_gas += tokens_in_calldata * STANDARD_TOKEN_COST;

    // Get number of access list account and storages.
    gas.initial_gas += access_list_accounts * ACCESS_LIST_ADDRESS;
    gas.initial_gas += access_list_storages * ACCESS_LIST_STORAGE_KEY;

    // Base stipend
    gas.initial_gas += if is_create {
        if spec_id.is_enabled_in(SpecId::HOMESTEAD) {
            // EIP-2: Homestead Hard-fork Changes
            53000
        } else {
            21000
        }
    } else {
        21000
    };

    // EIP-3860: Limit and meter initcode
    // Init code stipend for bytecode analysis
    if spec_id.is_enabled_in(SpecId::SHANGHAI) && is_create {
        gas.initial_gas += initcode_cost(input.len())
    }

    // EIP-7702
    if spec_id.is_enabled_in(SpecId::PRAGUE) {
        gas.initial_gas += authorization_list_num * eip7702::PER_EMPTY_ACCOUNT_COST;

        // Calculate gas floor for EIP-7623
        gas.floor_gas = calc_tx_floor_cost(tokens_in_calldata);
    }

    gas
}

/// Initial gas that is deducted for transaction to be included.
/// Initial gas contains initial stipend gas, gas for access list and input data.
///
/// # Returns
///
/// - Intrinsic gas
/// - Number of tokens in calldata
pub fn calculate_initial_tx_gas_for_tx(tx: impl Transaction, spec: SpecId) -> InitialAndFloorGas {
    let mut accounts = 0;
    let mut storages = 0;
    // legacy is only tx type that does not have access list.
    if tx.tx_type() != TransactionType::Legacy {
        (accounts, storages) = tx
            .access_list()
            .map(|al| {
                al.fold((0, 0), |(mut num_accounts, mut num_storage_slots), item| {
                    num_accounts += 1;
                    num_storage_slots += item.storage_slots().count();

                    (num_accounts, num_storage_slots)
                })
            })
            .unwrap_or_default();
    }

    calculate_initial_tx_gas(
        spec,
        tx.input(),
        tx.kind().is_create(),
        accounts as u64,
        storages as u64,
        tx.authorization_list_len() as u64,
    )
}

/// Retrieve the total number of tokens in calldata.
#[inline]
pub fn get_tokens_in_calldata(input: &[u8], is_istanbul: bool) -> u64 {
    let zero_data_len = input.iter().filter(|v| **v == 0).count() as u64;
    let non_zero_data_len = input.len() as u64 - zero_data_len;
    let non_zero_data_multiplier = if is_istanbul {
        // EIP-2028: Transaction data gas cost reduction
        NON_ZERO_BYTE_MULTIPLIER_ISTANBUL
    } else {
        NON_ZERO_BYTE_MULTIPLIER
    };
    zero_data_len + non_zero_data_len * non_zero_data_multiplier
}

/// Calculate the transaction cost floor as specified in EIP-7623.
#[inline]
pub fn calc_tx_floor_cost(tokens_in_calldata: u64) -> u64 {
    tokens_in_calldata * TOTAL_COST_FLOOR_PER_TOKEN + 21_000
}
