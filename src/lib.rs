//! # Medusa — POC Kernel Game Cheat Driver
//!
//! A proof-of-concept Windows kernel driver written in Rust that provides
//! arbitrary read/write access to process memory via a device interface.
//! Built on top of the windows-drivers-rs crate ecosystem.

#![no_std]
extern crate alloc;

#[cfg(not(test))]
extern crate wdk_panic;

mod coms;
mod string_stuff;
mod util;

use wdk::println;

#[cfg(not(test))]
use wdk_alloc::WdkAllocator;
use wdk_sys::{DRIVER_OBJECT, NTSTATUS, PCUNICODE_STRING, STATUS_SUCCESS};

#[cfg(not(test))]
#[global_allocator]
static GLOBAL_ALLOCATOR: WdkAllocator = WdkAllocator;

/// `DriverEntry` — called by Windows when the driver is loaded.
///
/// Sets up the device object and registers IRP dispatch handlers.
///
/// # Safety
/// Dereferences raw pointers passed from the Windows kernel.
#[export_name = "DriverEntry"]
pub unsafe extern "system" fn driver_entry(
    driver: &mut DRIVER_OBJECT,
    _registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    println!("[medusa] [i] Driver loading...");
    driver.DriverUnload = Some(driver_exit);

    let status = unsafe { coms::setup_device(driver) };
    if status != STATUS_SUCCESS {
        println!("[medusa] [-] setup_device failed: {status:#x}");
        return status;
    }

    driver.MajorFunction[wdk_sys::IRP_MJ_CREATE as usize] = Some(coms::dispatch_create_close);
    driver.MajorFunction[wdk_sys::IRP_MJ_CLOSE as usize] = Some(coms::dispatch_create_close);
    driver.MajorFunction[wdk_sys::IRP_MJ_READ as usize] = Some(coms::dispatch_read);
    driver.MajorFunction[wdk_sys::IRP_MJ_WRITE as usize] = Some(coms::dispatch_write);

    println!("[medusa] [+] Driver loaded successfully");
    STATUS_SUCCESS
}

/// Called by Windows when the driver is unloaded.
extern "C" fn driver_exit(driver: *mut DRIVER_OBJECT) {
    // SAFETY: driver pointer is valid during unload callback
    unsafe {
        if let Some(driver_ref) = driver.as_mut() {
            coms::remove_device(driver_ref);
        }
    }
    println!("[medusa] [i] Driver unloaded");
}
