
use std::ffi::{c_void, OsStr};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null_mut;

const PACKAGE_FAMILY: &str =
    "0308c356-fa3a-4929-a83c-d7b7e59f5f16_yc2qef1tvgt8w";

const PIPE_NAME: &str = "MicroBoostGameBar";

#[repr(C)]
struct SecurityAttributes {
    length: u32,
    descriptor: *mut c_void,
    inherit_handle: i32,
}

#[link(name = "userenv")]
extern "system" {
    fn DeriveAppContainerSidFromAppContainerName(
        name: *const u16,
        sid: *mut *mut c_void,
    ) -> i32;
}

#[link(name = "advapi32")]
extern "system" {
    fn ConvertSidToStringSidW(
        sid: *mut c_void,
        string: *mut *mut u16,
    ) -> i32;

    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        text: *const u16,
        revision: u32,
        descriptor: *mut *mut c_void,
        size: *mut u32,
    ) -> i32;

    fn FreeSid(sid: *mut c_void) -> *mut c_void;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcessId() -> u32;

    fn ProcessIdToSessionId(
        process_id: u32,
        session_id: *mut u32,
    ) -> i32;

    fn LocalFree(memory: *mut c_void) -> *mut c_void;

    fn CreateNamedPipeW(
        name: *const u16,
        open_mode: u32,
        pipe_mode: u32,
        instances: u32,
        out_size: u32,
        in_size: u32,
        timeout: u32,
        security: *const SecurityAttributes,
    ) -> *mut c_void;

    fn ConnectNamedPipe(
        pipe: *mut c_void,
        overlapped: *mut c_void,
    ) -> i32;

    fn ReadFile(
        file: *mut c_void,
        buffer: *mut c_void,
        size: u32,
        read: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;

    fn WriteFile(
        file: *mut c_void,
        buffer: *const c_void,
        size: u32,
        written: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;

    fn CloseHandle(handle: *mut c_void) -> i32;

    fn GetLastError() -> u32;
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn current_session_id() -> io::Result<u32> {
    let mut session_id = 0u32;

    let ok = unsafe {
        ProcessIdToSessionId(
            GetCurrentProcessId(),
            &mut session_id,
        )
    };

    if ok == 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(session_id)
}

fn package_sid() -> io::Result<String> {
    let mut sid = null_mut();

    let result = unsafe {
        DeriveAppContainerSidFromAppContainerName(
            wide(PACKAGE_FAMILY).as_ptr(),
            &mut sid,
        )
    };

    if result != 0 {
        return Err(io::Error::other(format!(
            "Cannot derive package SID: 0x{:08X}",
            result as u32
        )));
    }

    let mut text: *mut u16 = null_mut();

    let converted = unsafe {
        ConvertSidToStringSidW(sid, &mut text)
    };

    if converted == 0 {
        let error = io::Error::last_os_error();

        unsafe {
            FreeSid(sid);
        }

        return Err(error);
    }

    let mut length = 0usize;

    unsafe {
        while *text.add(length) != 0 {
            length += 1;
        }
    }

    let result = unsafe {
        String::from_utf16_lossy(
            std::slice::from_raw_parts(
                text,
                length,
            ),
        )
    };

    unsafe {
        LocalFree(text.cast());
        FreeSid(sid);
    }

    Ok(result)
}

fn main() -> io::Result<()> {
    println!("=================================");
    println!("   MICROBOOST UWP PIPE PROBE");
    println!("=================================");
    println!();

    let sid = package_sid()?;
    let session = current_session_id()?;

    // UWP LOCAL pipes resolve inside the
    // AppContainer-specific named-object namespace.
    let pipe_path = format!(
        r"\\?\pipe\Sessions\{}\AppContainerNamedObjects\{}\{}",
        session,
        sid,
        PIPE_NAME
    );

    println!("Session ID: {}", session);
    println!("Package SID: {}", sid);
    println!("Pipe path: {}", pipe_path);
    println!();

    // Grant pipe read/write access to the specific
    // AppContainer package, with low integrity.
    let security_text = format!(
        "D:(A;;GA;;;SY)(A;;GA;;;OW)\
         (A;;GRGW;;;{})S:(ML;;NW;;;LW)",
        sid
    );

    let mut descriptor = null_mut();

    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide(&security_text).as_ptr(),
            1,
            &mut descriptor,
            null_mut(),
        )
    };

    if converted == 0 {
        return Err(io::Error::last_os_error());
    }

    let attributes = SecurityAttributes {
        length: std::mem::size_of::<SecurityAttributes>()
            as u32,
        descriptor,
        inherit_handle: 0,
    };

    const PIPE_ACCESS_DUPLEX: u32 = 0x00000003;
    const FILE_FLAG_FIRST_PIPE_INSTANCE: u32 = 0x00080000;
    const PIPE_REJECT_REMOTE_CLIENTS: u32 = 0x00000008;

    let pipe = unsafe {
        CreateNamedPipeW(
            wide(&pipe_path).as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_REJECT_REMOTE_CLIENTS,
            1,
            4096,
            4096,
            0,
            &attributes,
        )
    };

    unsafe {
        LocalFree(descriptor);
    }

    if pipe as isize == -1 {
        return Err(io::Error::last_os_error());
    }

    println!("Pipe created successfully.");
    println!("Waiting for UWP client...");

    let connected = unsafe {
        ConnectNamedPipe(pipe, null_mut())
    };

    if connected == 0 {
        let error = unsafe { GetLastError() };

        // ERROR_PIPE_CONNECTED
        if error != 535 {
            unsafe {
                CloseHandle(pipe);
            }

            return Err(
                io::Error::from_raw_os_error(error as i32)
            );
        }
    }

    println!("Client connected!");

    let mut buffer = [0u8; 256];
    let mut bytes_read = 0u32;

    let read_ok = unsafe {
        ReadFile(
            pipe,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
            &mut bytes_read,
            null_mut(),
        )
    };

    if read_ok == 0 {
        let error = io::Error::last_os_error();

        unsafe {
            CloseHandle(pipe);
        }

        return Err(error);
    }

    let received = String::from_utf8_lossy(
        &buffer[..bytes_read as usize]
    );

    println!("Received: {}", received.trim());

    let response: &[u8] =
        if received.trim() == "PING" {
            b"PONG\n"
        } else {
            b"UNKNOWN\n"
        };

    let mut written = 0u32;

    let write_ok = unsafe {
        WriteFile(
            pipe,
            response.as_ptr().cast(),
            response.len() as u32,
            &mut written,
            null_mut(),
        )
    };

    let write_error = if write_ok == 0 {
        Some(io::Error::last_os_error())
    } else {
        None
    };

    unsafe {
        CloseHandle(pipe);
    }

    if let Some(error) = write_error {
        return Err(error);
    }

    println!("Response sent.");
    println!("Test complete.");

    Ok(())
}
