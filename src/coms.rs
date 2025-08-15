use core::ptr::null_mut;
use wdk::nt_success;
use wdk::println;

use core::ffi::c_void;
use wdk_sys::DO_BUFFERED_IO;
use wdk_sys::IRP_MJ_CLOSE;
use wdk_sys::IRP_MJ_CREATE;
use wdk_sys::IRP_MJ_READ;
use wdk_sys::IRP_MJ_WRITE;
use wdk_sys::PAGE_GUARD;
use wdk_sys::PIO_STACK_LOCATION;
use wdk_sys::PIRP;
use wdk_sys::{
    DRIVER_OBJECT, FILE_DEVICE_SECURE_OPEN, FILE_DEVICE_UNKNOWN, NTSTATUS, PDEVICE_OBJECT,
    STATUS_SUCCESS, STATUS_UNSUCCESSFUL,
};
use wdk_sys::ntddk::{IoCreateDevice, IoCreateSymbolicLink, IoDeleteDevice, IoDeleteSymbolicLink};

use crate::string_stuff::{ToUnicodeString, ToWindowsUnicodeString};

use wdk_sys::ntddk::*;
use wdk_sys::{
    IRP, IO_STACK_LOCATION, STATUS_INVALID_PARAMETER, STATUS_BUFFER_TOO_SMALL,
    IO_NO_INCREMENT,
};


pub unsafe fn IoGetCurrentIrpStackLocation(irp: PIRP) -> PIO_STACK_LOCATION {
    unsafe {
        assert!((*irp).CurrentLocation <= (*irp).StackCount + 1); // todo maybe do error handling instead of an assert?
        (*irp).Tail.Overlay.__bindgen_anon_2.__bindgen_anon_1.CurrentStackLocation
    }
}

pub unsafe fn setup_device(driver: &mut DRIVER_OBJECT) -> NTSTATUS {
    let mut dos_name = "\\DosDevices\\Medusa"
        .to_u16_vec()
        .to_windows_unicode_string()
        .expect("[Medusa] [-] Unable to encode DOS name.");

    let mut nt_name = "\\Device\\Medusa"
        .to_u16_vec()
        .to_windows_unicode_string()
        .expect("[Medusa] [-] Unable to encode NT name.");

    let mut device_object: PDEVICE_OBJECT = null_mut();

    let res = unsafe {
        IoCreateDevice(
            driver,
            0,
            &mut nt_name,
            FILE_DEVICE_UNKNOWN,
            FILE_DEVICE_SECURE_OPEN,
            0,
            &mut device_object,
        )
    };

    if !nt_success(res) {
        println!("[Medusa] [-] IoCreateDevice failed: {res:#x}");
        return res;
    }
    
    (*driver).DeviceObject = device_object;
    unsafe {
        (*device_object).Flags |= DO_BUFFERED_IO;
    }

    let res = unsafe { IoCreateSymbolicLink(&mut dos_name, &mut nt_name) };
    if !nt_success(res) {
        println!("[Medusa] [-] IoCreateSymbolicLink failed: {res:#x}");
        unsafe {
            IoDeleteDevice(device_object);
        }
        return STATUS_UNSUCCESSFUL;
    }

    STATUS_SUCCESS
}

pub unsafe fn remove_device(driver: &mut DRIVER_OBJECT) -> NTSTATUS {
    let mut dos_name = "\\DosDevices\\Medusa"
        .to_u16_vec()
        .to_windows_unicode_string()
        .expect("[Medusa] [-] Unable to encode DOS name.");

    let _ = unsafe { IoDeleteSymbolicLink(&mut dos_name) };

    if !driver.DeviceObject.is_null() {
        unsafe {
            IoDeleteDevice(driver.DeviceObject);
        }
    }

    STATUS_SUCCESS
}


// --- new globals ----------------------------------------------------------
static mut BUFFER:     [u8; 256] = [0; 256];
static mut BUFFER_LEN: usize      = 0;

static mut RESP:       [u8; 256] = [0; 256];
static mut RESP_LEN:   usize      = 0;
// --------------------------------------------------------------------------

pub unsafe extern "C" fn dispatch(
    _dev: PDEVICE_OBJECT,
    irp: *mut IRP,
) -> NTSTATUS {
    unsafe {
        

        let stack   = IoGetCurrentIrpStackLocation(irp);
        let major   = (*stack).MajorFunction as u32;
        println!("[medusa] dispatch major: {:#x}", major);
        let sys_buf = (*irp).AssociatedIrp.SystemBuffer as *mut u8;

        match major {
            IRP_MJ_CREATE | IRP_MJ_CLOSE => {
                println!("[medusa] IRP_MJ_CREATE / IRP_MJ_CLOSE");
                (*irp).IoStatus.__bindgen_anon_1.Status = STATUS_SUCCESS;
                (*irp).IoStatus.Information = 0;
                IofCompleteRequest(irp, IO_NO_INCREMENT as _);
                return STATUS_SUCCESS;
            }

            IRP_MJ_WRITE => {
                let wlen = (*stack).Parameters.Write.Length.min(256) as usize;
                core::ptr::copy_nonoverlapping(sys_buf, BUFFER.as_mut_ptr(), wlen);
                BUFFER_LEN = wlen;

                println!("[medusa] IRP_MJ_WRITE, wlen: {wlen}, buffer: {:?}", &BUFFER[..wlen]);
                let irql = KeGetCurrentIrql();
                println!("[medusa] IRQL before MmCopyVirtualMemory: {irql}");
                if wlen >= 17 && &BUFFER[8..13] == b"write" {
                    // Parse address from BUFFER[0..8]
                    let user_address = usize::from_le_bytes([
                        BUFFER[0], BUFFER[1], BUFFER[2], BUFFER[3],
                        BUFFER[4], BUFFER[5], BUFFER[6], BUFFER[7],
                    ]) as *mut core::ffi::c_void;

                    // Parse PID from BUFFER[13..17]
                    let target_pid = u32::from_le_bytes([
                        BUFFER[13], BUFFER[14], BUFFER[15], BUFFER[16],
                    ]);

                    // Data follows from byte 17
                    let data = &BUFFER[17..wlen];
                    let mut data_buf = [0u8; 240];
                    data_buf[..data.len()].copy_from_slice(data);

                    println!("[medusa] write command received");
                    println!("[medusa] PID={target_pid}, addr={user_address:?}, data={data:?}");

                    match crate::util::copy_usermode_memory(target_pid, user_address, &mut data_buf[..data.len()],true) {
                        Ok(written) => {
                            println!("[medusa] write to user mode successful, written: {written}");
                            RESP[..4].copy_from_slice(b"ok");
                            RESP_LEN = 2;
                        }
                        Err(status) => {
                            println!("[medusa] write to user mode failed: {status:#x}");
                            RESP[..4].copy_from_slice(b"fail");
                            RESP_LEN = 4;
                        }
                    }
                }

                (*irp).IoStatus.__bindgen_anon_1.Status = STATUS_SUCCESS;
                (*irp).IoStatus.Information = wlen as u64;
                IofCompleteRequest(irp, IO_NO_INCREMENT as _);
                return STATUS_SUCCESS;
            }

            IRP_MJ_READ => {
                let rlen = (*stack).Parameters.Read.Length.min(RESP_LEN as u32) as usize;
                core::ptr::copy_nonoverlapping(RESP.as_ptr(), sys_buf, rlen);

                (*irp).IoStatus.__bindgen_anon_1.Status = STATUS_SUCCESS;
                (*irp).IoStatus.Information = rlen as u64;
                IofCompleteRequest(irp, IO_NO_INCREMENT as _);
                return STATUS_SUCCESS;
            }

            _ => {
                (*irp).IoStatus.__bindgen_anon_1.Status = STATUS_INVALID_PARAMETER;
                (*irp).IoStatus.Information = 0;
                IofCompleteRequest(irp, IO_NO_INCREMENT as _);
                return STATUS_INVALID_PARAMETER;
            }
        }
    }
    
    STATUS_SUCCESS
}
