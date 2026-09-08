//! Deterministic, side-effect-free Phase 3 host-pool accounting and arbitration.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Host pressure bands used by global arbitration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HostPressureState {
    Normal,
    Caution,
    Pressure,
    Critical,
    Emergency,
}

/// Immutable host capacity inputs for one arbitration cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlobalPoolConfig {
    pub total_physical_bytes: u64,
    pub fixed_host_baseline_bytes: u64,
    pub cache_allowance_bytes: u64,
    pub emergency_reserve_bytes: u64,
    pub report_max_age_millis: u64,
    pub previous_pressure: Option<HostPressureState>,
}

/// One VM's fresh demand and authoritative live allocation state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmPoolInput {
    pub vm_name: String,
    pub actual_allocation_bytes: u64,
    pub requested_allocation_bytes: u64,
    pub minimum_bytes: u64,
    pub maximum_bytes: u64,
    pub desired_target_bytes: u64,
    pub safe_floor_bytes: u64,
    pub block_size_bytes: u64,
    pub report_observed_unix_millis: u64,
    pub growth_priority: u32,
    pub reclaim_priority: u32,
}

/// A simulation result; it never authorizes or performs actuation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum GlobalPoolDecision {
    Hold {
        vm_name: String,
        reason: &'static str,
    },
    Grow {
        vm_name: String,
        target_bytes: u64,
    },
    Reclaim {
        vm_name: String,
        target_bytes: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalPoolPlan {
    pub pressure: HostPressureState,
    pub allocatable_pool_bytes: u64,
    pub actual_allocated_bytes: u64,
    pub pool_free_bytes: u64,
    pub decisions: Vec<GlobalPoolDecision>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GlobalPoolError {
    #[error("global pool configuration is invalid: {0}")]
    InvalidConfig(&'static str),
    #[error("VM identity is empty or duplicated: {0}")]
    InvalidVmIdentity(String),
    #[error("VM {vm} has invalid or unaligned memory geometry: {reason}")]
    InvalidVmGeometry { vm: String, reason: &'static str },
    #[error("VM {vm} demand report is stale or from the future")]
    StaleReport { vm: String },
    #[error("global pool accounting overflowed")]
    ArithmeticOverflow,
    #[error("actual VM allocations exceed the configured host pool")]
    OvercommittedPool,
}

/// Computes an atomic plan from a complete snapshot of all VM inputs.
pub fn arbitrate_global_pool(
    config: GlobalPoolConfig,
    now_unix_millis: u64,
    vms: &[VmPoolInput],
) -> Result<GlobalPoolPlan, GlobalPoolError> {
    validate_config(config)?;
    let reserved = config
        .fixed_host_baseline_bytes
        .checked_add(config.cache_allowance_bytes)
        .and_then(|value| value.checked_add(config.emergency_reserve_bytes))
        .ok_or(GlobalPoolError::ArithmeticOverflow)?;
    let allocatable_pool_bytes =
        config
            .total_physical_bytes
            .checked_sub(reserved)
            .ok_or(GlobalPoolError::InvalidConfig(
                "host reservations exceed physical RAM",
            ))?;

    let mut actual_allocated_bytes = 0_u64;
    for (index, vm) in vms.iter().enumerate() {
        validate_vm(vm, now_unix_millis, config.report_max_age_millis)?;
        if vms[..index].iter().any(|other| other.vm_name == vm.vm_name) {
            return Err(GlobalPoolError::InvalidVmIdentity(vm.vm_name.clone()));
        }
        actual_allocated_bytes = actual_allocated_bytes
            .checked_add(vm.actual_allocation_bytes)
            .ok_or(GlobalPoolError::ArithmeticOverflow)?;
    }
    let pool_free_bytes = allocatable_pool_bytes
        .checked_sub(actual_allocated_bytes)
        .ok_or(GlobalPoolError::OvercommittedPool)?;
    let pressure = transition_pressure(
        config.previous_pressure,
        pool_free_bytes,
        allocatable_pool_bytes,
    );

    let mut decisions = vms
        .iter()
        .map(|vm| GlobalPoolDecision::Hold {
            vm_name: vm.vm_name.clone(),
            reason: if vm.requested_allocation_bytes != vm.actual_allocation_bytes {
                "in_flight"
            } else {
                "no_change"
            },
        })
        .collect::<Vec<_>>();

    if matches!(
        pressure,
        HostPressureState::Normal | HostPressureState::Caution
    ) {
        let mut free = pool_free_bytes;
        let mut order = (0..vms.len()).collect::<Vec<_>>();
        order.sort_by(|left, right| {
            vms[*right]
                .growth_priority
                .cmp(&vms[*left].growth_priority)
                .then_with(|| vms[*left].vm_name.cmp(&vms[*right].vm_name))
        });
        for index in order {
            let vm = &vms[index];
            if vm.requested_allocation_bytes != vm.actual_allocation_bytes
                || vm.desired_target_bytes <= vm.actual_allocation_bytes
            {
                continue;
            }
            let wanted = vm.desired_target_bytes.min(vm.maximum_bytes);
            let available_target = vm.actual_allocation_bytes.saturating_add(free);
            let target = align_down(wanted.min(available_target), vm.block_size_bytes);
            if target > vm.actual_allocation_bytes {
                free -= target - vm.actual_allocation_bytes;
                decisions[index] = GlobalPoolDecision::Grow {
                    vm_name: vm.vm_name.clone(),
                    target_bytes: target,
                };
            }
        }
    } else {
        let normal_free = allocatable_pool_bytes
            .checked_add(3)
            .ok_or(GlobalPoolError::ArithmeticOverflow)?
            / 4;
        let mut reclaim_needed = normal_free.saturating_sub(pool_free_bytes);
        let mut order = (0..vms.len()).collect::<Vec<_>>();
        order.sort_by(|left, right| {
            vms[*right]
                .reclaim_priority
                .cmp(&vms[*left].reclaim_priority)
                .then_with(|| vms[*left].vm_name.cmp(&vms[*right].vm_name))
        });
        for index in order {
            let vm = &vms[index];
            if vm.requested_allocation_bytes != vm.actual_allocation_bytes {
                continue;
            }
            let floor = vm.safe_floor_bytes.max(vm.minimum_bytes);
            let reclaimable = vm.actual_allocation_bytes.saturating_sub(floor);
            if reclaimable > 0 && reclaim_needed > 0 {
                let desired_reclaim = align_up(reclaim_needed, vm.block_size_bytes)?;
                let reclaim = reclaimable.min(desired_reclaim);
                decisions[index] = GlobalPoolDecision::Reclaim {
                    vm_name: vm.vm_name.clone(),
                    target_bytes: vm.actual_allocation_bytes - reclaim,
                };
                reclaim_needed = reclaim_needed.saturating_sub(reclaim);
            }
        }
    }

    Ok(GlobalPoolPlan {
        pressure,
        allocatable_pool_bytes,
        actual_allocated_bytes,
        pool_free_bytes,
        decisions,
    })
}

fn validate_config(config: GlobalPoolConfig) -> Result<(), GlobalPoolError> {
    if config.total_physical_bytes == 0 || config.report_max_age_millis == 0 {
        return Err(GlobalPoolError::InvalidConfig(
            "physical RAM and report age must be positive",
        ));
    }
    Ok(())
}

fn validate_vm(vm: &VmPoolInput, now: u64, max_age: u64) -> Result<(), GlobalPoolError> {
    if vm.vm_name.trim().is_empty() || vm.vm_name.len() > 128 || !vm.vm_name.is_ascii() {
        return Err(GlobalPoolError::InvalidVmIdentity(vm.vm_name.clone()));
    }
    if vm.block_size_bytes == 0
        || !vm.block_size_bytes.is_power_of_two()
        || vm.minimum_bytes > vm.maximum_bytes
        || vm.actual_allocation_bytes < vm.minimum_bytes
        || vm.actual_allocation_bytes > vm.maximum_bytes
        || vm.desired_target_bytes < vm.minimum_bytes
        || vm.desired_target_bytes > vm.maximum_bytes
        || vm.safe_floor_bytes < vm.minimum_bytes
        || vm.safe_floor_bytes > vm.actual_allocation_bytes
    {
        return Err(GlobalPoolError::InvalidVmGeometry {
            vm: vm.vm_name.clone(),
            reason: "values are outside configured bounds",
        });
    }
    for value in [
        vm.actual_allocation_bytes,
        vm.requested_allocation_bytes,
        vm.minimum_bytes,
        vm.maximum_bytes,
        vm.desired_target_bytes,
        vm.safe_floor_bytes,
    ] {
        if !value.is_multiple_of(vm.block_size_bytes) {
            return Err(GlobalPoolError::InvalidVmGeometry {
                vm: vm.vm_name.clone(),
                reason: "values must be block aligned",
            });
        }
    }
    if vm.report_observed_unix_millis > now
        || now.saturating_sub(vm.report_observed_unix_millis) > max_age
    {
        return Err(GlobalPoolError::StaleReport {
            vm: vm.vm_name.clone(),
        });
    }
    Ok(())
}

fn align_down(value: u64, alignment: u64) -> u64 {
    value - (value % alignment)
}

fn align_up(value: u64, alignment: u64) -> Result<u64, GlobalPoolError> {
    let remainder = value % alignment;
    if remainder == 0 {
        Ok(value)
    } else {
        value
            .checked_add(alignment - remainder)
            .ok_or(GlobalPoolError::ArithmeticOverflow)
    }
}

fn classify_pressure(free: u64, capacity: u64) -> HostPressureState {
    let basis_points = free.saturating_mul(10_000) / capacity.max(1);
    match basis_points {
        2_500.. => HostPressureState::Normal,
        1_500..=2_499 => HostPressureState::Caution,
        750..=1_499 => HostPressureState::Pressure,
        250..=749 => HostPressureState::Critical,
        _ => HostPressureState::Emergency,
    }
}

fn transition_pressure(
    previous: Option<HostPressureState>,
    free: u64,
    capacity: u64,
) -> HostPressureState {
    let raw = classify_pressure(free, capacity);
    let Some(previous) = previous else {
        return raw;
    };
    if pressure_rank(raw) >= pressure_rank(previous) {
        return raw;
    }
    let basis_points = free.saturating_mul(10_000) / capacity.max(1);
    let recovery_threshold = match previous {
        HostPressureState::Normal => 0,
        HostPressureState::Caution => 2_700,
        HostPressureState::Pressure => 1_700,
        HostPressureState::Critical => 950,
        HostPressureState::Emergency => 450,
    };
    if basis_points >= recovery_threshold {
        raw
    } else {
        previous
    }
}

fn pressure_rank(state: HostPressureState) -> u8 {
    match state {
        HostPressureState::Normal => 0,
        HostPressureState::Caution => 1,
        HostPressureState::Pressure => 2,
        HostPressureState::Critical => 3,
        HostPressureState::Emergency => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    fn config() -> GlobalPoolConfig {
        GlobalPoolConfig {
            total_physical_bytes: 64 * GIB,
            fixed_host_baseline_bytes: 8 * GIB,
            cache_allowance_bytes: 4 * GIB,
            emergency_reserve_bytes: 4 * GIB,
            report_max_age_millis: 60_000,
            previous_pressure: None,
        }
    }

    fn vm(name: &str, actual: u64, desired: u64, growth: u32, reclaim: u32) -> VmPoolInput {
        VmPoolInput {
            vm_name: name.to_owned(),
            actual_allocation_bytes: actual,
            requested_allocation_bytes: actual,
            minimum_bytes: 4 * GIB,
            maximum_bytes: 32 * GIB,
            desired_target_bytes: desired,
            safe_floor_bytes: actual.min(6 * GIB),
            block_size_bytes: 2 * GIB,
            report_observed_unix_millis: 1_000_000,
            growth_priority: growth,
            reclaim_priority: reclaim,
        }
    }

    #[test]
    fn accounts_actual_not_requested_and_suppresses_in_flight_vm() {
        let mut second = vm("b", 8 * GIB, 12 * GIB, 100, 1);
        second.requested_allocation_bytes = 10 * GIB;
        let plan = arbitrate_global_pool(
            config(),
            1_001_000,
            &[vm("a", 8 * GIB, 10 * GIB, 1, 1), second],
        )
        .expect("valid pool");
        assert_eq!(plan.actual_allocated_bytes, 16 * GIB);
        assert_eq!(plan.pool_free_bytes, 32 * GIB);
        assert_eq!(
            plan.decisions[1],
            GlobalPoolDecision::Hold {
                vm_name: "b".to_owned(),
                reason: "in_flight"
            }
        );
    }

    #[test]
    fn growth_is_atomic_bounded_aligned_and_priority_ordered() {
        let config = GlobalPoolConfig {
            total_physical_bytes: 28 * GIB,
            ..config()
        };
        let plan = arbitrate_global_pool(
            config,
            1_000_000,
            &[
                vm("low", 4 * GIB, 8 * GIB, 1, 1),
                vm("high", 4 * GIB, 10 * GIB, 10, 1),
            ],
        )
        .expect("valid pool");
        assert_eq!(plan.pool_free_bytes, 4 * GIB);
        assert_eq!(
            plan.decisions[0],
            GlobalPoolDecision::Hold {
                vm_name: "low".to_owned(),
                reason: "no_change"
            }
        );
        assert_eq!(
            plan.decisions[1],
            GlobalPoolDecision::Grow {
                vm_name: "high".to_owned(),
                target_bytes: 8 * GIB
            }
        );
    }

    #[test]
    fn pressure_reclaims_to_safe_floor_in_priority_order() {
        let config = GlobalPoolConfig {
            total_physical_bytes: 34 * GIB,
            ..config()
        };
        let plan = arbitrate_global_pool(
            config,
            1_000_000,
            &[
                vm("a", 8 * GIB, 8 * GIB, 1, 9),
                vm("b", 8 * GIB, 8 * GIB, 1, 1),
            ],
        )
        .expect("valid pool");
        assert_eq!(plan.pressure, HostPressureState::Pressure);
        assert_eq!(
            plan.decisions[0],
            GlobalPoolDecision::Reclaim {
                vm_name: "a".to_owned(),
                target_bytes: 6 * GIB
            }
        );
        assert_eq!(
            plan.decisions[1],
            GlobalPoolDecision::Reclaim {
                vm_name: "b".to_owned(),
                target_bytes: 6 * GIB
            }
        );
    }

    #[test]
    fn classifies_all_five_pressure_states() {
        assert_eq!(classify_pressure(25, 100), HostPressureState::Normal);
        assert_eq!(classify_pressure(15, 100), HostPressureState::Caution);
        assert_eq!(classify_pressure(8, 100), HostPressureState::Pressure);
        assert_eq!(classify_pressure(3, 100), HostPressureState::Critical);
        assert_eq!(classify_pressure(2, 100), HostPressureState::Emergency);
    }

    #[test]
    fn pressure_recovery_uses_hysteresis_while_deterioration_is_immediate() {
        assert_eq!(
            transition_pressure(Some(HostPressureState::Caution), 26, 100),
            HostPressureState::Caution
        );
        assert_eq!(
            transition_pressure(Some(HostPressureState::Caution), 27, 100),
            HostPressureState::Normal
        );
        assert_eq!(
            transition_pressure(Some(HostPressureState::Normal), 24, 100),
            HostPressureState::Caution
        );
    }

    #[test]
    fn rejects_stale_duplicate_unaligned_and_overcommitted_inputs() {
        let mut stale = vm("a", 8 * GIB, 8 * GIB, 1, 1);
        stale.report_observed_unix_millis = 900_000;
        assert!(matches!(
            arbitrate_global_pool(config(), 1_000_000, &[stale]),
            Err(GlobalPoolError::StaleReport { .. })
        ));
        let duplicate = vm("same", 8 * GIB, 8 * GIB, 1, 1);
        assert!(matches!(
            arbitrate_global_pool(config(), 1_000_000, &[duplicate.clone(), duplicate]),
            Err(GlobalPoolError::InvalidVmIdentity(_))
        ));
        let mut unaligned = vm("a", 8 * GIB, 8 * GIB, 1, 1);
        unaligned.desired_target_bytes += 1;
        assert!(matches!(
            arbitrate_global_pool(config(), 1_000_000, &[unaligned]),
            Err(GlobalPoolError::InvalidVmGeometry { .. })
        ));
        assert_eq!(
            arbitrate_global_pool(
                GlobalPoolConfig {
                    total_physical_bytes: 24 * GIB,
                    ..config()
                },
                1_000_000,
                &[vm("a", 10 * GIB, 10 * GIB, 1, 1)]
            ),
            Err(GlobalPoolError::OvercommittedPool)
        );
    }
}
