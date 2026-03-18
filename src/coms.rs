//! Device communication layer.
//!
//! Handles IRP dispatch for the `\\Device\\Medusa` kernel device.
//! Supports read and write commands via buffered I/O.
//!
//! ## Wire Protocol (IRP_MJ_WRITE)
//!
//! Commands are sent as raw bytes via `WriteFile()`:
//!
//! | Offset | Size | Field             |
//! |--------|------|-------------------|
//! | 0      | 8    | Target address    |
//! | 8      | 5    | Command tag       |
//! | 13     | 4    | Target PID        |
//! | 17     | N    | Payload (write)   |
//!
//! Command tags: `"write"` or `"read\0"`.
//!
//! Responses are read back via `ReadFile()` from the internal response buffer.

use core::ptr::null_mut;

use wdk::println;
use wdk::{nt_success};

use wdk_sys::DO_BUFFERED_IO;
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
    IRP, STATUS_INVALID_PARAMETER,
    IO_NO_INCREMENT,
};

/// Maximum buffer size for IRP data exchange.
const BUFFER_SIZE: usize = 4096;

/// Minimum command size: 8 (address) + 5 (tag) + 4 (PID) = 17 bytes.
const MIN_CMD_SIZE: usize = 17;

/// Command tag for write operations.
const CMD_WRITE: &[u8] = b"write";

/// Command tag for read operations (null-padded to 5 bytes).
const CMD_READ: &[u8] = b"read\0";

// ---------------------------------------------------------------------------
// Global buffers (POC only — not safe for concurrent access)
// ---------------------------------------------------------------------------
// SAFETY: These are only accessed from dispatch routines running at
// PASSIVE_LEVEL. In a production driver you would use a proper
// synchronisation primitive (e.g. KMUTEX).
static mut BUFFER: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
static mut BUFFER_LEN: usize = 0;

static mut RESP: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
static mut RESP_LEN: usize = 0;

/// Retrieves the current IRP stack location.
///
/// # Safety
/// `irp` must be a valid, non-null pointer to an `IRP`.
pub unsafe fn io_get_current_irp_stack_location(irp: PIRP) -> PIO_STACK_LOCATION {
    unsafe {
        assert!((*irp).CurrentLocation <= (*irp).StackCount + 1);
        (*irp).Tail.Overlay.__bindgen_anon_2.__bindgen_anon_1.CurrentStackLocation
    }
}

/// Creates the `\\Device\\Medusa` device and `\\DosDevices\\Medusa` symlink.
///
/// # Safety
/// `driver` must be a valid reference to the calling `DRIVER_OBJECT`.
pub unsafe fn setup_device(driver: &mut DRIVER_OBJECT) -> NTSTATUS {
    let mut dos_name = "\\DosDevices\\Medusa"
        .to_u16_vec()
        .to_windows_unicode_string()
        .expect("[medusa] [-] Unable to encode DOS name.");

    let mut nt_name = "\\Device\\Medusa"
        .to_u16_vec()
        .to_windows_unicode_string()
        .expect("[medusa] [-] Unable to encode NT name.");

    let mut device_object: PDEVICE_OBJECT = null_mut();

    // SAFETY: IoCreateDevice is called with valid driver and output pointer
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
        println!("[medusa] [-] IoCreateDevice failed: {res:#x}");
        return res;
    }

    (*driver).DeviceObject = device_object;
    // SAFETY: device_object was just successfully created
    unsafe {
        (*device_object).Flags |= DO_BUFFERED_IO;
    }

    // SAFETY: IoCreateSymbolicLink is called with valid UNICODE_STRINGs
    let res = unsafe { IoCreateSymbolicLink(&mut dos_name, &mut nt_name) };
    if !nt_success(res) {
        println!("[medusa] [-] IoCreateSymbolicLink failed: {res:#x}");
        // SAFETY: device_object is valid and must be cleaned up on failure
        unsafe {
            IoDeleteDevice(device_object);
        }
        return STATUS_UNSUCCESSFUL;
    }

    STATUS_SUCCESS
}

/// Removes the device and symbolic link during driver unload.
///
/// # Safety
/// `driver` must be a valid reference to the calling `DRIVER_OBJECT`.
pub unsafe fn remove_device(driver: &mut DRIVER_OBJECT) -> NTSTATUS {
    let mut dos_name = "\\DosDevices\\Medusa"
        .to_u16_vec()
        .to_windows_unicode_string()
        .expect("[medusa] [-] Unable to encode DOS name.");

    // SAFETY: Symlink was created during setup_device
    let _ = unsafe { IoDeleteSymbolicLink(&mut dos_name) };

    if !driver.DeviceObject.is_null() {
        // SAFETY: DeviceObject is non-null and was created by this driver
        unsafe {
            IoDeleteDevice(driver.DeviceObject);
        }
    }

    STATUS_SUCCESS
}

// ---------------------------------------------------------------------------
// IRP dispatch handlers
// ---------------------------------------------------------------------------

/// Handles IRP_MJ_CREATE and IRP_MJ_CLOSE — simply completes the request.
///
/// # Safety
/// Called by the I/O manager with valid device and IRP pointers.
pub unsafe extern "C" fn dispatch_create_close(
    _dev: PDEVICE_OBJECT,
    irp: *mut IRP,
) -> NTSTATUS {
    // SAFETY: irp is valid, provided by the I/O manager
    unsafe {
        (*irp).IoStatus.__bindgen_anon_1.Status = STATUS_SUCCESS;
        (*irp).IoStatus.Information = 0;
        IofCompleteRequest(irp, IO_NO_INCREMENT as _);
    }
    STATUS_SUCCESS
}

/// Handles IRP_MJ_READ — returns the response buffer contents.
///
/// # Safety
/// Called by the I/O manager with valid device and IRP pointers.
pub unsafe extern "C" fn dispatch_read(
    _dev: PDEVICE_OBJECT,
    irp: *mut IRP,
) -> NTSTATUS {
    // SAFETY: irp and stack location are valid
    unsafe {
        let stack = io_get_current_irp_stack_location(irp);
        let sys_buf = (*irp).AssociatedIrp.SystemBuffer as *mut u8;

        if sys_buf.is_null() {
            (*irp).IoStatus.__bindgen_anon_1.Status = STATUS_INVALID_PARAMETER;
            (*irp).IoStatus.Information = 0;
            IofCompleteRequest(irp, IO_NO_INCREMENT as _);
            return STATUS_INVALID_PARAMETER;
        }

        let rlen = ((*stack).Parameters.Read.Length as usize).min(RESP_LEN);
        // SAFETY: sys_buf is valid buffered I/O pointer, RESP is valid static
        core::ptr::copy_nonoverlapping(RESP.as_ptr(), sys_buf, rlen);

        (*irp).IoStatus.__bindgen_anon_1.Status = STATUS_SUCCESS;
        (*irp).IoStatus.Information = rlen as u64;
        IofCompleteRequest(irp, IO_NO_INCREMENT as _);
    }
    STATUS_SUCCESS
}

/// Handles IRP_MJ_WRITE — parses and executes read/write commands.
///
/// # Safety
/// Called by the I/O manager with valid device and IRP pointers.
pub unsafe extern "C" fn dispatch_write(
    _dev: PDEVICE_OBJECT,
    irp: *mut IRP,
) -> NTSTATUS {
    // SAFETY: irp and stack location are valid
    unsafe {
        let stack = io_get_current_irp_stack_location(irp);
        let sys_buf = (*irp).AssociatedIrp.SystemBuffer as *mut u8;

        if sys_buf.is_null() {
            (*irp).IoStatus.__bindgen_anon_1.Status = STATUS_INVALID_PARAMETER;
            (*irp).IoStatus.Information = 0;
            IofCompleteRequest(irp, IO_NO_INCREMENT as _);
            return STATUS_INVALID_PARAMETER;
        }

        let wlen = ((*stack).Parameters.Write.Length as usize).min(BUFFER_SIZE);
        // SAFETY: sys_buf is valid buffered I/O, BUFFER is valid static
        core::ptr::copy_nonoverlapping(sys_buf, BUFFER.as_mut_ptr(), wlen);
        BUFFER_LEN = wlen;

        // Parse command if we have enough bytes
        if wlen >= MIN_CMD_SIZE {
            let cmd_tag = &BUFFER[8..13];

            // Parse address (bytes 0..8, little-endian u64)
            let user_address = usize::from_le_bytes([
                BUFFER[0], BUFFER[1], BUFFER[2], BUFFER[3],
                BUFFER[4], BUFFER[5], BUFFER[6], BUFFER[7],
            ]) as *mut core::ffi::c_void;

            // Parse PID (bytes 13..17, little-endian u32)
            let target_pid = u32::from_le_bytes([
                BUFFER[13], BUFFER[14], BUFFER[15], BUFFER[16],
            ]);

            // Validate address is non-null
            if user_address.is_null() {
                println!("[medusa] [-] Null target address");
                RESP[..4].copy_from_slice(b"fail");
                RESP_LEN = 4;
            } else if cmd_tag == CMD_WRITE {
                // --- WRITE: kernel → target process ---
                let data = &BUFFER[MIN_CMD_SIZE..wlen];
                let mut data_buf = [0u8; BUFFER_SIZE - MIN_CMD_SIZE];
                let copy_len = data.len().min(data_buf.len());
                data_buf[..copy_len].copy_from_slice(&data[..copy_len]);

                println!("[medusa] write cmd: PID={target_pid}, addr={user_address:?}, len={copy_len}");

                match crate::util::copy_usermode_memory(target_pid, user_address, &mut data_buf[..copy_len], true) {
                    Ok(written) => {
                        println!("[medusa] [+] Write OK, {written} bytes");
                        // Store the number of bytes written as response
                        let written_bytes = (written as u32).to_le_bytes();
                        RESP[..2].copy_from_slice(b"ok");
                        RESP[2..6].copy_from_slice(&written_bytes);
                        RESP_LEN = 6;
                    }
                    Err(status) => {
                        println!("[medusa] [-] Write failed: {status:#x}");
                        RESP[..4].copy_from_slice(b"fail");
                        RESP_LEN = 4;
                    }
                }
            } else if cmd_tag == CMD_READ {
                // --- READ: target process → kernel → userland via ReadFile ---
                // Bytes 17..21 contain the read size (u32 LE)
                if wlen < MIN_CMD_SIZE + 4 {
                    println!("[medusa] [-] Read command too short");
                    RESP[..4].copy_from_slice(b"fail");
                    RESP_LEN = 4;
                } else {
                    let read_size = u32::from_le_bytes([
                        BUFFER[17], BUFFER[18], BUFFER[19], BUFFER[20],
                    ]) as usize;

                    let clamped = read_size.min(BUFFER_SIZE - 2); // leave room for "ok" prefix
                    let mut read_buf = [0u8; BUFFER_SIZE];

                    println!("[medusa] read cmd: PID={target_pid}, addr={user_address:?}, len={clamped}");

                    match crate::util::copy_usermode_memory(target_pid, user_address, &mut read_buf[..clamped], false) {
                        Ok(read) => {
                            println!("[medusa] [+] Read OK, {read} bytes");
                            RESP[..2].copy_from_slice(b"ok");
                            RESP[2..2 + read].copy_from_slice(&read_buf[..read]);
                            RESP_LEN = 2 + read;
                        }
                        Err(status) => {
                            println!("[medusa] [-] Read failed: {status:#x}");
                            RESP[..4].copy_from_slice(b"fail");
                            RESP_LEN = 4;
                        }
                    }
                }
            } else {
                println!("[medusa] [-] Unknown command tag: {:?}", cmd_tag);
                RESP[..4].copy_from_slice(b"fail");
                RESP_LEN = 4;
            }
        }

        (*irp).IoStatus.__bindgen_anon_1.Status = STATUS_SUCCESS;
        (*irp).IoStatus.Information = wlen as u64;
        IofCompleteRequest(irp, IO_NO_INCREMENT as _);
    }
    STATUS_SUCCESS
}
