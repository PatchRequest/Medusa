// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! # Sample WDM Driver
//!
//! This is a sample WDM driver that demonstrates how to use the crates in
//! windows-driver-rs to create a skeleton of a WDM driver.

#![no_std]
extern crate alloc;

#[cfg(not(test))]
extern crate wdk_panic;
mod string_stuff;
mod util;
mod coms;
use wdk::println;

#[cfg(not(test))]
use wdk_alloc::WdkAllocator;
use wdk_sys::{DRIVER_OBJECT, NTSTATUS, PCUNICODE_STRING, STATUS_SUCCESS};

#[cfg(not(test))]
#[global_allocator]
static GLOBAL_ALLOCATOR: WdkAllocator = WdkAllocator;



/// `driver_entry` function required by WDM
///
/// # Panics
/// Can panic from unwraps of `CStrings` used internally
///
/// # Safety
/// Function is unsafe since it dereferences raw pointers passed to it from WDM
#[export_name = "DriverEntry"]
pub unsafe extern "system" fn driver_entry(
    driver: &mut DRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    println!("[medusa] [i] Hello world!");
    driver.DriverUnload = Some(driver_exit);

    unsafe {
         let status = unsafe { coms::setup_device(driver) };
        if status != STATUS_SUCCESS {
            println!("[medusa] [-] setup_device failed: {status:#x}");
            return status;
        }
    }
    driver.MajorFunction[wdk_sys::IRP_MJ_CREATE as usize] = Some(coms::dispatch);
    driver.MajorFunction[wdk_sys::IRP_MJ_CLOSE as usize]  = Some(coms::dispatch);
    driver.MajorFunction[wdk_sys::IRP_MJ_READ as usize] = Some(coms::dispatch);
    driver.MajorFunction[wdk_sys::IRP_MJ_WRITE as usize] = Some(coms::dispatch);
        

    STATUS_SUCCESS
}

extern "C" fn driver_exit(driver: *mut DRIVER_OBJECT) {

       // rm symbolic link
    unsafe {
        if let Some(driver_ref) = driver.as_mut() {
            coms::remove_device(driver_ref);
        }
    }

    println!("[medusa] driver unloaded successfully...");
}
