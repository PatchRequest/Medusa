#![no_std]

use core::{ffi::c_void, ptr};
use wdk_sys::ntddk::{
    IoAllocateMdl, IoFreeMdl, IoGetCurrentProcess, KeStackAttachProcess, KeUnstackDetachProcess, MmMapLockedPagesSpecifyCache, MmUnlockPages, MmUnmapLockedPages, ObfDereferenceObject, PsLookupProcessByProcessId             // Correct function to dereference
};
use wdk_sys::_MEMORY_CACHING_TYPE::MmCached;
use wdk_sys::_MM_PAGE_PRIORITY::NormalPagePriority;
use wdk_sys::_MODE::KernelMode;
use wdk_sys::{FALSE, KAPC_STATE, NTSTATUS,NT_SUCCESS, STATUS_ACCESS_VIOLATION, STATUS_INSUFFICIENT_RESOURCES, STATUS_SUCCESS};
use wdk::println;


#[repr(C)]
#[allow(non_camel_case_types)]
pub enum KPROCESSOR_MODE {
    KernelMode = 0,
    UserMode = 1,
}

extern "system" {
    pub fn MmCopyVirtualMemory(
        from_process: *mut c_void,
        from_address: *const c_void,
        to_process: *mut c_void,
        to_address: *mut c_void,
        buffer_size: usize,
        previous_mode: KPROCESSOR_MODE,
        return_size: *mut usize,
    ) -> NTSTATUS;
}

pub fn copy_usermode_memory(
    target_pid: u32,
    user_address: *mut c_void,
    buffer: &mut [u8],
    is_write: bool,
) -> Result<usize, NTSTATUS> {
    unsafe {
        let mut target_process: *mut c_void = ptr::null_mut();

        println!(
            "[medusa] Resolving PEPROCESS for PID: {target_pid}, operation: {}",
            if is_write { "WRITE" } else { "READ" }
        );

        let status = PsLookupProcessByProcessId(
            target_pid as _,
            &mut target_process as *mut _ as *mut _,
        );

        if status != 0 {
            println!("[medusa] PsLookupProcessByProcessId failed: 0x{status:08X}");
            return Err(status);
        }

        let this_process = IoGetCurrentProcess();
        let mut transferred: usize = 0;

        let (from_process, from_address, to_process, to_address) = if is_write {
            (
                this_process as *mut c_void,
                buffer.as_ptr() as *mut c_void,
                target_process,
                user_address,
            )
        } else {
            (
                target_process,
                user_address,
                this_process as *mut c_void,
                buffer.as_mut_ptr() as *mut c_void,
            )
        };

        println!(
            "[medusa] MmCopyVirtualMemory {}:\n\
             \t→ from PID: {:?}, addr: {:?}\n\
             \t→ to   PID: {:?}, addr: {:?}\n\
             \t→ len: {}",
            if is_write { "WRITE" } else { "READ" },
            from_process,
            from_address,
            to_process,
            to_address,
            buffer.len()
        );

        let status = MmCopyVirtualMemory(
            from_process as _,
            from_address,
            to_process as _,
            to_address,
            buffer.len(),
            KPROCESSOR_MODE::KernelMode,
            &mut transferred,
        );

        ObfDereferenceObject(target_process);

        println!(
            "[medusa] MmCopyVirtualMemory returned: 0x{status:08X}, transferred: {transferred}"
        );

        if NT_SUCCESS(status) {
            Ok(transferred)
        } else {
            Err(status)
        }
    }
}
