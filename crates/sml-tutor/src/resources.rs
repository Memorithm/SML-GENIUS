//! Physical resource accounting for the scheduler.
//!
//! This module is pure bookkeeping. It knows nothing about hardware: machine
//! capacities are declared by the caller, and a machine is only admitted for
//! learning work when its capacity is marked `measured`. Unmeasured machines
//! fail closed.
//!
//! Only [`MachineRole::AiCompute`] machines can ever receive a [`Lease`].
//! Build machines (for example a reserved Mac mini) and control-plane hosts
//! are rejected regardless of their free capacity.

use std::collections::BTreeMap;
use std::fmt;

/// A quantity of physical resources.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Resources {
    /// CPU in thousandths of a core.
    pub cpu_milli: u64,
    /// Number of whole GPU slots.
    pub gpu_slots: u32,
    /// System memory in MiB.
    pub ram_mib: u64,
    /// GPU memory in MiB.
    pub vram_mib: u64,
    /// Scratch storage in MiB.
    pub storage_mib: u64,
}

impl Resources {
    /// True when every component of `self` is at most the matching component of `avail`.
    pub fn fits_in(&self, avail: &Resources) -> bool {
        self.cpu_milli <= avail.cpu_milli
            && self.gpu_slots <= avail.gpu_slots
            && self.ram_mib <= avail.ram_mib
            && self.vram_mib <= avail.vram_mib
            && self.storage_mib <= avail.storage_mib
    }

    /// Component-wise subtraction; `None` if any component would underflow.
    pub fn checked_sub(&self, rhs: &Resources) -> Option<Resources> {
        Some(Resources {
            cpu_milli: self.cpu_milli.checked_sub(rhs.cpu_milli)?,
            gpu_slots: self.gpu_slots.checked_sub(rhs.gpu_slots)?,
            ram_mib: self.ram_mib.checked_sub(rhs.ram_mib)?,
            vram_mib: self.vram_mib.checked_sub(rhs.vram_mib)?,
            storage_mib: self.storage_mib.checked_sub(rhs.storage_mib)?,
        })
    }

    /// Component-wise addition that clamps at the numeric maximum.
    pub fn saturating_add(&self, rhs: &Resources) -> Resources {
        Resources {
            cpu_milli: self.cpu_milli.saturating_add(rhs.cpu_milli),
            gpu_slots: self.gpu_slots.saturating_add(rhs.gpu_slots),
            ram_mib: self.ram_mib.saturating_add(rhs.ram_mib),
            vram_mib: self.vram_mib.saturating_add(rhs.vram_mib),
            storage_mib: self.storage_mib.saturating_add(rhs.storage_mib),
        }
    }
}

/// What a machine is allowed to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineRole {
    /// May receive learning tasks.
    AiCompute,
    /// Reserved for iOS builds (for example a Mac mini). Never receives learning tasks.
    IosBuildReserved,
    /// Runs the scheduler and control plane. Never receives learning tasks.
    ControlPlane,
}

/// A machine known to the resource manager.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Machine {
    pub name: String,
    pub role: MachineRole,
    pub capacity: Resources,
    /// True only when `capacity` comes from a real measurement.
    pub measured: bool,
    pub online: bool,
}

/// A reservation of resources on one machine. Must be passed back to
/// [`ResourceManager::release`] exactly once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lease {
    pub id: u64,
    pub machine: String,
    pub demand: Resources,
}

/// Errors returned by the resource manager.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceError {
    /// Two machines share a name.
    DuplicateMachine(String),
    /// No machine with this name is registered.
    UnknownMachine(String),
    /// No AiCompute machine is both online and measured.
    NoEligibleMachine,
    /// Eligible machines exist but none has enough free capacity. This is the backpressure signal.
    Insufficient,
    /// The lease was never issued by this manager, or was already released.
    UnknownLease(u64),
    /// Lease identifiers are exhausted.
    IdExhausted,
}

impl fmt::Display for ResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResourceError::DuplicateMachine(n) => write!(f, "duplicate machine name: {n}"),
            ResourceError::UnknownMachine(n) => write!(f, "unknown machine: {n}"),
            ResourceError::NoEligibleMachine => {
                write!(f, "no online, measured AI-compute machine is registered")
            }
            ResourceError::Insufficient => {
                write!(f, "no eligible machine has enough free capacity")
            }
            ResourceError::UnknownLease(id) => {
                write!(f, "unknown or already released lease {id}")
            }
            ResourceError::IdExhausted => write!(f, "lease identifiers are exhausted"),
        }
    }
}

impl std::error::Error for ResourceError {}

#[derive(Debug)]
struct Slot {
    machine: Machine,
    free: Resources,
}

/// Accounts for leases over a fixed set of machines.
#[derive(Debug)]
pub struct ResourceManager {
    slots: BTreeMap<String, Slot>,
    leases: BTreeMap<u64, Lease>,
    next_id: u64,
}

fn is_eligible(m: &Machine) -> bool {
    m.role == MachineRole::AiCompute && m.measured && m.online
}

impl ResourceManager {
    /// Registers the machines. Every machine starts with its full capacity free.
    pub fn new(machines: Vec<Machine>) -> Result<Self, ResourceError> {
        let mut slots = BTreeMap::new();
        for m in machines {
            if slots.contains_key(&m.name) {
                return Err(ResourceError::DuplicateMachine(m.name));
            }
            let free = m.capacity;
            slots.insert(m.name.clone(), Slot { machine: m, free });
        }
        Ok(ResourceManager {
            slots,
            leases: BTreeMap::new(),
            next_id: 1,
        })
    }

    /// Reserves `demand` on the eligible machine with the most CPU left after
    /// allocation (ties go to the lexicographically smallest name).
    pub fn acquire(&mut self, demand: Resources) -> Result<Lease, ResourceError> {
        let mut any_eligible = false;
        let mut best: Option<(u64, &str)> = None;
        for slot in self.slots.values() {
            if !is_eligible(&slot.machine) {
                continue;
            }
            any_eligible = true;
            let Some(after) = slot.free.checked_sub(&demand) else {
                continue;
            };
            let name = slot.machine.name.as_str();
            match best {
                Some((cpu, _)) if cpu >= after.cpu_milli => {}
                _ => best = Some((after.cpu_milli, name)),
            }
        }
        let Some((_, chosen)) = best else {
            return Err(if any_eligible {
                ResourceError::Insufficient
            } else {
                ResourceError::NoEligibleMachine
            });
        };
        let chosen = chosen.to_owned();

        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(ResourceError::IdExhausted)?;
        let Some(slot) = self.slots.get_mut(&chosen) else {
            return Err(ResourceError::NoEligibleMachine);
        };
        slot.free = slot
            .free
            .checked_sub(&demand)
            .ok_or(ResourceError::Insufficient)?;
        let lease = Lease {
            id,
            machine: chosen,
            demand,
        };
        self.leases.insert(id, lease.clone());
        Ok(lease)
    }

    /// Returns a lease's resources to its machine. Works even if the machine
    /// has since gone offline. A second release of the same lease is an error.
    pub fn release(&mut self, lease: &Lease) -> Result<(), ResourceError> {
        match self.leases.get(&lease.id) {
            Some(held) if held == lease => {}
            _ => return Err(ResourceError::UnknownLease(lease.id)),
        }
        let Some(slot) = self.slots.get_mut(&lease.machine) else {
            return Err(ResourceError::UnknownLease(lease.id));
        };
        slot.free = slot.free.saturating_add(&lease.demand);
        self.leases.remove(&lease.id);
        Ok(())
    }

    /// Marks a machine online or offline. An offline machine keeps its existing
    /// leases accounted until they are released, but receives no new leases.
    pub fn set_online(&mut self, name: &str, online: bool) -> Result<(), ResourceError> {
        let slot = self
            .slots
            .get_mut(name)
            .ok_or_else(|| ResourceError::UnknownMachine(name.to_owned()))?;
        slot.machine.online = online;
        Ok(())
    }

    /// Currently free capacity on a machine, or `None` if it is not registered.
    pub fn free(&self, name: &str) -> Option<Resources> {
        self.slots.get(name).map(|s| s.free)
    }

    /// Number of leases issued and not yet released.
    pub fn active_leases(&self) -> usize {
        self.leases.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn res(cpu: u64, gpu: u32, ram: u64, vram: u64) -> Resources {
        Resources {
            cpu_milli: cpu,
            gpu_slots: gpu,
            ram_mib: ram,
            vram_mib: vram,
            storage_mib: 0,
        }
    }

    fn machine(name: &str, role: MachineRole, capacity: Resources, measured: bool) -> Machine {
        Machine {
            name: name.to_owned(),
            role,
            capacity,
            measured,
            online: true,
        }
    }

    fn small(demand_cpu: u64) -> Resources {
        res(demand_cpu, 0, 256, 0)
    }

    #[test]
    fn ios_build_machine_is_never_selected() {
        let mac = machine(
            "mac-mini",
            MachineRole::IosBuildReserved,
            res(1_000_000, 0, 1_000_000, 0),
            true,
        );
        let ctl = machine(
            "control",
            MachineRole::ControlPlane,
            res(1_000_000, 0, 1_000_000, 0),
            true,
        );
        let ai = machine(
            "ai-1",
            MachineRole::AiCompute,
            res(2_000, 0, 4_096, 0),
            true,
        );
        let mut rm = ResourceManager::new(vec![mac, ctl, ai]).unwrap();
        for _ in 0..3 {
            let lease = rm.acquire(small(500)).unwrap();
            assert_eq!(lease.machine, "ai-1");
        }
        assert_eq!(rm.free("mac-mini"), Some(res(1_000_000, 0, 1_000_000, 0)));
        assert_eq!(rm.free("control"), Some(res(1_000_000, 0, 1_000_000, 0)));
    }

    #[test]
    fn only_non_ai_machines_registered_means_no_eligible_machine() {
        let mac = machine(
            "mac-mini",
            MachineRole::IosBuildReserved,
            res(8_000, 0, 8_192, 0),
            true,
        );
        let mut rm = ResourceManager::new(vec![mac]).unwrap();
        assert_eq!(
            rm.acquire(small(100)),
            Err(ResourceError::NoEligibleMachine)
        );
    }

    #[test]
    fn unmeasured_machine_is_rejected() {
        let guess = machine(
            "guess",
            MachineRole::AiCompute,
            res(64_000, 8, 65_536, 80_000),
            false,
        );
        let mut rm = ResourceManager::new(vec![guess]).unwrap();
        assert_eq!(
            rm.acquire(small(100)),
            Err(ResourceError::NoEligibleMachine)
        );
        assert_eq!(rm.active_leases(), 0);
    }

    #[test]
    fn gpu_demand_routes_only_to_gpu_machine() {
        let cpu_box = machine(
            "cpu-box",
            MachineRole::AiCompute,
            res(64_000, 0, 65_536, 0),
            true,
        );
        let gpu_box = machine(
            "gpu-box",
            MachineRole::AiCompute,
            res(8_000, 2, 32_768, 24_576),
            true,
        );
        let mut rm = ResourceManager::new(vec![cpu_box, gpu_box]).unwrap();
        let demand = res(1_000, 1, 1_024, 8_192);
        let lease = rm.acquire(demand).unwrap();
        assert_eq!(lease.machine, "gpu-box");
        assert_eq!(rm.free("gpu-box").unwrap().gpu_slots, 1);
        assert_eq!(rm.free("cpu-box").unwrap().gpu_slots, 0);
    }

    #[test]
    fn insufficient_when_full_then_succeeds_after_release() {
        let ai = machine(
            "ai-1",
            MachineRole::AiCompute,
            res(1_000, 0, 1_024, 0),
            true,
        );
        let mut rm = ResourceManager::new(vec![ai]).unwrap();
        let first = rm.acquire(small(800)).unwrap();
        assert_eq!(rm.acquire(small(800)), Err(ResourceError::Insufficient));
        rm.release(&first).unwrap();
        let second = rm.acquire(small(800)).unwrap();
        assert_eq!(second.machine, "ai-1");
    }

    #[test]
    fn double_release_is_an_error() {
        let ai = machine(
            "ai-1",
            MachineRole::AiCompute,
            res(1_000, 0, 1_024, 0),
            true,
        );
        let mut rm = ResourceManager::new(vec![ai]).unwrap();
        let lease = rm.acquire(small(100)).unwrap();
        rm.release(&lease).unwrap();
        assert_eq!(
            rm.release(&lease),
            Err(ResourceError::UnknownLease(lease.id))
        );
    }

    #[test]
    fn forged_lease_is_rejected() {
        let ai = machine(
            "ai-1",
            MachineRole::AiCompute,
            res(1_000, 0, 1_024, 0),
            true,
        );
        let mut rm = ResourceManager::new(vec![ai]).unwrap();
        let lease = rm.acquire(small(100)).unwrap();
        let forged = Lease {
            id: lease.id,
            machine: lease.machine.clone(),
            demand: small(999),
        };
        assert_eq!(
            rm.release(&forged),
            Err(ResourceError::UnknownLease(lease.id))
        );
        assert_eq!(rm.active_leases(), 1);
        rm.release(&lease).unwrap();
    }

    #[test]
    fn offline_machine_gets_no_new_lease_but_keeps_accounting() {
        let a = machine("a", MachineRole::AiCompute, res(1_000, 0, 1_024, 0), true);
        let b = machine("b", MachineRole::AiCompute, res(1_000, 0, 1_024, 0), true);
        let mut rm = ResourceManager::new(vec![a, b]).unwrap();
        let on_a = rm.acquire(small(900)).unwrap();
        assert_eq!(on_a.machine, "a");
        rm.set_online("a", false).unwrap();
        let on_b = rm.acquire(small(900)).unwrap();
        assert_eq!(on_b.machine, "b");
        assert_eq!(rm.free("a"), Some(res(100, 0, 768, 0)));
        // The offline machine still honours release of its existing lease.
        rm.release(&on_a).unwrap();
        assert_eq!(rm.free("a"), Some(res(1_000, 0, 1_024, 0)));
        rm.set_online("b", false).unwrap();
        assert_eq!(rm.acquire(small(1)), Err(ResourceError::NoEligibleMachine));
        assert_eq!(
            rm.set_online("zzz", true),
            Err(ResourceError::UnknownMachine("zzz".to_owned()))
        );
    }

    #[test]
    fn duplicate_names_are_rejected() {
        let a = machine("dup", MachineRole::AiCompute, res(1, 0, 1, 0), true);
        let b = machine("dup", MachineRole::AiCompute, res(1, 0, 1, 0), true);
        assert_eq!(
            ResourceManager::new(vec![a, b]).unwrap_err(),
            ResourceError::DuplicateMachine("dup".to_owned())
        );
    }

    #[test]
    fn free_capacity_is_restored_exactly_after_release() {
        let capacity = res(4_000, 2, 8_192, 16_384);
        let ai = machine("ai-1", MachineRole::AiCompute, capacity, true);
        let mut rm = ResourceManager::new(vec![ai]).unwrap();
        let before = rm.free("ai-1").unwrap();
        assert_eq!(before, capacity);
        let l1 = rm.acquire(res(1_500, 1, 2_048, 4_096)).unwrap();
        let l2 = rm.acquire(res(500, 1, 1_024, 8_192)).unwrap();
        assert_eq!(rm.active_leases(), 2);
        rm.release(&l2).unwrap();
        rm.release(&l1).unwrap();
        assert_eq!(rm.free("ai-1"), Some(before));
        assert_eq!(rm.active_leases(), 0);
    }

    #[test]
    fn resources_arithmetic_edges() {
        let a = res(10, 1, 10, 10);
        let b = res(11, 0, 0, 0);
        assert!(!b.fits_in(&a));
        assert!(a.fits_in(&a));
        assert_eq!(a.checked_sub(&b), None);
        assert_eq!(b.checked_sub(&a), None);
        assert_eq!(
            a.checked_sub(&res(4, 1, 5, 0)),
            Some(Resources {
                cpu_milli: 6,
                gpu_slots: 0,
                ram_mib: 5,
                vram_mib: 10,
                storage_mib: 0
            })
        );
        let max = Resources {
            cpu_milli: u64::MAX,
            gpu_slots: u32::MAX,
            ram_mib: u64::MAX,
            vram_mib: u64::MAX,
            storage_mib: u64::MAX,
        };
        assert_eq!(max.saturating_add(&a), max);
    }
}
