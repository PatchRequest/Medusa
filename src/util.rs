//! Utility functions for cross-process memory operations.
//!
//! Wraps `MmCopyVirtualMemory` to provide a safe(r) Rust interface for
//! reading from and writing to another process's virtual address space.

use core::{ffi::c_void, ptr};

use wdk::println;
use wdk_sys::ntddk::{IoGetCurrentProcess, ObfDereferenceObject, PsLookupProcessByProcessId};
use wdk_sys::{NTSTATUS, NT_SUCCESS};

/// Kernel/User processor mode for `MmCopyVirtualMemory`.
#[repr(C)]
#[allow(non_camel_case_types)]
pub enum KPROCESSOR_MODE {
    /// Kernel mode — no access checks.
    KernelMode = 0,
    /// User mode — standard access checks apply.
    UserMode = 1,
}

extern "system" {
    /// Undocumented NT kernel API for copying memory between processes.
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

/// Copies memory between the current (driver) process and a target process.
///
/// When `is_write` is `true`, data flows **from** `buffer` **to** the target
/// process at `user_address`. When `false`, data flows the other way.
///
/// # Arguments
/// * `target_pid`   — Process ID of the target process.
/// * `user_address` — Virtual address in the target process. Must be non-null.
/// * `buffer`       — Local kernel buffer to read from / write to.
/// * `is_write`     — Direction of the copy.
///
/// # Returns
/// `Ok(bytes_transferred)` on success, `Err(NTSTATUS)` on failure.
///
/// # Safety
/// This function is inherently unsafe — it performs cross-process memory
/// access at kernel level. The caller must ensure `user_address` is valid
/// in the target process and `buffer` is large enough.
pub fn copy_usermode_memory(
    target_pid: u32,
    user_address: *mut c_void,
    buffer: &mut [u8],
    is_write: bool,
) -> Result<usize, NTSTATUS> {
    // Validate inputs
    if user_address.is_null() {
        println!("[medusa] [-] copy_usermode_memory: null user_address");
        return Err(wdk_sys::STATUS_ACCESS_VIOLATION);
    }

    if buffer.is_empty() {
        println!("[medusa] [-] copy_usermode_memory: empty buffer");
        return Err(wdk_sys::STATUS_INVALID_PARAMETER);
    }

    unsafe {
        let mut target_process: *mut c_void = ptr::null_mut();

        println!(
            "[medusa] Resolving PEPROCESS for PID: {target_pid}, op: {}",
            if is_write { "WRITE" } else { "READ" }
        );

        // SAFETY: PsLookupProcessByProcessId is a documented NT API
        let status = PsLookupProcessByProcessId(
            target_pid as _,
            &mut target_process as *mut _ as *mut _,
        );

        if status != 0 {
            println!("[medusa] [-] PsLookupProcessByProcessId failed: 0x{status:08X}");
            return Err(status);
        }

        // SAFETY: IoGetCurrentProcess returns the current PEPROCESS
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
            "[medusa] MmCopyVirtualMemory: from={from_process:?} to={to_process:?} len={}",
            buffer.len()
        );

        // SAFETY: All process handles are valid, addresses are checked above
        let status = MmCopyVirtualMemory(
            from_process as _,
            from_address,
            to_process as _,
            to_address,
            buffer.len(),
            KPROCESSOR_MODE::KernelMode,
            &mut transferred,
        );

        // SAFETY: We hold a reference from PsLookupProcessByProcessId
        ObfDereferenceObject(target_process);

        println!(
            "[medusa] MmCopyVirtualMemory result: 0x{status:08X}, transferred: {transferred}"
        );

        if NT_SUCCESS(status) {
            Ok(transferred)
        } else {
            Err(status)
        }
    }
}
