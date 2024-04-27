pub mod allocator;
mod amd;
mod apic_id;
pub mod gdt_tss;
mod host;
mod intel;
pub mod paging_structures;
pub mod panic;
pub mod platform_ops;
mod registers;
mod segment;
mod serial_logger;
mod support;
mod switch_stack;
mod x86_instructions;

use alloc::{boxed::Box, vec::Vec};
use spin::Once;
use x86::cpuid::cpuid;

use crate::{hypervisor::registers::Registers, GdtTss, PagingStructures};

/// Hyperjacks the current system by virtualizing all logical processors on this
/// system.
pub fn virtualize_system(hv_data: SharedData) {
    serial_logger::init(log::LevelFilter::Debug);
    log::info!("Virtualizing the all processors");

    apic_id::init();
    let _ = SHARED_HV_DATA.call_once(|| hv_data);

    // Virtualize each logical processor.
    platform_ops::get().run_on_all_processors(|| {
        // Take a snapshot of current register values. This will be the initial
        // state of the guest _including RIP_. This means that the guest starts execution
        // right after this function call. Think of it as the setjmp() C standard
        // function.
        let registers = Registers::capture_current();

        // In the first run, our hypervisor is not installed and the branch is
        // taken. After starting the guest, the second run, the hypervisor is already
        // installed and we will bail out.
        if !is_our_hypervisor_present() {
            log::info!("Virtualizing the current processor");

            // We are about to execute hypervisor code with newly allocated stack.
            // This is required because the guest will start executing with the
            // current stack. If we do not change the stack for the hypervisor, as soon
            // as the guest starts, it will smash hypervisor's stack.
            switch_stack::jump_with_new_stack(host::main, &registers);
        }
        log::info!("Virtualized the current processor");
    });

    log::info!("Virtualized the all processors");
}

/// A collection of data that the hypervisor depends on for its entire lifespan.
#[derive(Debug, Default)]
pub struct SharedData {
    /// The paging structures for the hypervisor. If `None`, the current paging
    /// structure is used for both the hypervisor and the guest.
    pub host_pt: Option<PagingStructures>,

    /// The IDT for the hypervisor for each logical processor. If `None`, the
    /// current IDTs are used for both the hypervisor and the guest.
    pub host_idt: Option<Vec<u64>>,

    /// The GDT and TSS for the hypervisor for each logical processor. If `None`,
    /// the current GDTs and TSSes are used for both the hypervisor and the guest.
    pub host_gdt_and_tss: Option<Vec<Box<GdtTss>>>,
}

static SHARED_HV_DATA: Once<SharedData> = Once::new();

const HV_CPUID_VENDOR_AND_MAX_FUNCTIONS: u32 = 0x4000_0000;
const HV_CPUID_INTERFACE: u32 = 0x4000_0001;
const OUR_HV_VENDOR_NAME_EBX: u32 = u32::from_ne_bytes(*b"Bare");
const OUR_HV_VENDOR_NAME_ECX: u32 = u32::from_ne_bytes(*b"viso");
const OUR_HV_VENDOR_NAME_EDX: u32 = u32::from_ne_bytes(*b"r!  ");

fn is_our_hypervisor_present() -> bool {
    let regs = cpuid!(HV_CPUID_VENDOR_AND_MAX_FUNCTIONS);
    (regs.ebx == OUR_HV_VENDOR_NAME_EBX)
        && (regs.ecx == OUR_HV_VENDOR_NAME_ECX)
        && (regs.edx == OUR_HV_VENDOR_NAME_EDX)
}
