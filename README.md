# Medusa

A proof-of-concept (POC) Windows kernel driver written in Rust, designed as a game cheating framework. Medusa provides arbitrary read/write access to any process's memory from kernel space via a simple device I/O interface.

> **Disclaimer:** This project is for educational and research purposes only. Using kernel drivers to manipulate game memory may violate terms of service and applicable laws. Use responsibly.

## What It Does

Medusa loads as a Windows kernel driver (`medusa.sys`) and exposes a device at `\\.\Medusa`. A userland application communicates with it via standard `CreateFile` / `WriteFile` / `ReadFile` Win32 calls to read or write memory in any target process — bypassing usermode protections like `PAGE_GUARD` and anti-cheat hooks.

### Architecture

```
┌──────────────┐     WriteFile/ReadFile      ┌────────────────┐
│  Userland    │  ◄─────────────────────────► │  \\.\Medusa    │
│  Cheat App   │        DeviceIoControl       │  Kernel Driver │
└──────────────┘                              └───────┬────────┘
                                                      │
                                              MmCopyVirtualMemory
                                                      │
                                              ┌───────▼────────┐
                                              │  Target Game   │
                                              │  Process       │
                                              └────────────────┘
```

## Wire Protocol

Commands are sent as raw bytes via `WriteFile()`:

| Offset | Size    | Field                          |
|--------|---------|--------------------------------|
| 0      | 8 bytes | Target virtual address (u64 LE)|
| 8      | 5 bytes | Command tag (see below)        |
| 13     | 4 bytes | Target PID (u32 LE)            |
| 17     | N bytes | Payload (varies by command)    |

### Commands

**Write** (`write`): Writes payload bytes to the target address.
```
[address:8][write:5][pid:4][data:N]
```

**Read** (`read\0`): Reads memory from the target address. Payload contains the read size as u32 LE.
```
[address:8][read\0:5][pid:4][size:4]
```

### Responses (via `ReadFile`)

- Success: `ok` (2 bytes) + response data
- Failure: `fail` (4 bytes)

## Prerequisites

- Windows 10/11 with test signing enabled (`bcdedit /set testsigning on`)
- [WDK](https://learn.microsoft.com/en-us/windows-hardware/drivers/download-the-wdk) (Windows Driver Kit) or eWDK
- [LLVM](https://releases.llvm.org/) (for Rust kernel builds)
- [Rust](https://rustup.rs/) with the `nightly` toolchain
- [cargo-make](https://github.com/sagiegurari/cargo-make): `cargo install cargo-make`
- The [windows-drivers-rs](https://github.com/microsoft/windows-drivers-rs) repository (Medusa lives inside its tree)

## Build

```powershell
cargo make
```

The compiled driver will be at `target\debug\medusa.sys` (or `target\release\medusa.sys` for release builds).

## Sign

Sign the driver with a self-signed test certificate:

```powershell
.\sign.ps1
# or for release builds:
.\sign.ps1 -BuildProfile release
```

## Install

1. Enable test signing on the target machine:
   ```cmd
   bcdedit /set testsigning on
   ```
   Reboot.

2. Copy `medusa.sys`, `medusa.inx`, and the certificate files to the target machine.

3. Install the certificate:
   - Double-click `driver_cert.cer`
   - Install → Local Machine → Trusted Root Certification Authorities
   - Repeat for Trusted Publishers

4. Install the driver:
   ```cmd
   pnputil.exe /add-driver medusa.inx /install
   ```

5. Create the device node:
   ```cmd
   devgen.exe /add /hardwareid "root\SAMPLE_WDM_HW_ID"
   ```

## Debug Output

Use [DebugView](https://learn.microsoft.com/en-us/sysinternals/downloads/debugview) with "Capture Kernel" enabled, or attach WinDbg:

```
ed nt!Kd_DEFAULT_Mask 0xFFFFFFFF
```

All log lines are prefixed with `[medusa]`.

## Known Limitations

- **No synchronisation** on global buffers — this is a single-client POC, not production code
- **No IOCTL interface** — uses raw read/write IRP dispatch (simpler but less flexible)
- **Test-signed only** — requires test signing mode or a valid EV code signing certificate
- **x64 only** — address parsing assumes 8-byte pointers
