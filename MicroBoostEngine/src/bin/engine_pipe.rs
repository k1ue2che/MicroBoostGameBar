
#[path = "../audio.rs"]
mod audio;

use std::ffi::{c_void, OsStr};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null_mut;

const PACKAGE_FAMILY: &str =
    "0308c356-fa3a-4929-a83c-d7b7e59f5f16_yc2qef1tvgt8w";

const PIPE_NAME: &str = "MicroBoostGameBar";

const INVALID_HANDLE: isize = -1;
const ERROR_PIPE_CONNECTED: u32 = 535;

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
        unsafe { FreeSid(sid); }
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
            std::slice::from_raw_parts(text, length)
        )
    };

    unsafe {
        LocalFree(text.cast());
        FreeSid(sid);
    }

    Ok(result)
}

fn session_id() -> io::Result<u32> {
    let mut session = 0u32;

    let ok = unsafe {
        ProcessIdToSessionId(
            GetCurrentProcessId(),
            &mut session,
        )
    };

    if ok == 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(session)
}

fn process_command(
    command: &str,
    pipeline: &audio::AudioPipeline,
    current_gain: &mut u32,
) -> String {
    let command = command.trim();

    if command == "PING" {
        return "PONG\n".to_string();
    }

    if command == "GET_GAIN" {
        return format!("GAIN {}\n", current_gain);
    }

    if let Some(value) = command.strip_prefix("GAIN ") {
        if let Ok(percent) = value.trim().parse::<u32>() {
            if (10..=500).contains(&percent) {
                pipeline.set_gain(percent);
                *current_gain = percent;

                println!("Boost updated: {}%", percent);

                return format!("OK {}\n", percent);
            }
        }

        return "ERROR Invalid gain (10-500)\n".to_string();
    }

    "ERROR Unknown command\n".to_string()
}

fn serve_client(
    pipe: *mut c_void,
    pipeline: &audio::AudioPipeline,
    current_gain: &mut u32,
) -> io::Result<()> {
    let connected = unsafe {
        ConnectNamedPipe(pipe, null_mut())
    };

    if connected == 0 {
        let error = unsafe { GetLastError() };

        if error != ERROR_PIPE_CONNECTED {
            return Err(
                io::Error::from_raw_os_error(error as i32)
            );
        }
    }

    println!("Client connected.");

    let mut buffer = [0u8; 256];
    let mut bytes_read = 0u32;

    let ok = unsafe {
        ReadFile(
            pipe,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
            &mut bytes_read,
            null_mut(),
        )
    };

    if ok == 0 {
        return Err(io::Error::last_os_error());
    }

    if bytes_read == 0 {
        return Err(io::Error::other(
            "Client disconnected without a command"
        ));
    }

    let command = std::str::from_utf8(
        &buffer[..bytes_read as usize]
    )
    .map_err(|_| io::Error::other("Invalid UTF-8"))?;

    println!("Command: {}", command.trim());

    let response =
        process_command(command, pipeline, current_gain);

    let bytes = response.as_bytes();
    let mut written = 0u32;

    let ok = unsafe {
        WriteFile(
            pipe,
            bytes.as_ptr().cast(),
            bytes.len() as u32,
            &mut written,
            null_mut(),
        )
    };

    if ok == 0 {
        return Err(io::Error::last_os_error());
    }

    if written != bytes.len() as u32 {
        return Err(io::Error::other(
            "Incomplete response"
        ));
    }

    println!("Response: {}", response.trim());

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=================================");
    println!("   MICROBOOST AUDIO + IPC");
    println!("=================================");

    // Start the already tested audio pipeline.
    let pipeline = audio::AudioPipeline::start()?;
    let mut current_gain = 100u32;

    println!();
    println!("Starting Game Bar communication...");

    let sid = package_sid()?;
    let session = session_id()?;

    let pipe_path = format!(
        r"\\?\pipe\Sessions\{}\AppContainerNamedObjects\{}\{}",
        session,
        sid,
        PIPE_NAME
    );

    // Reuse the security configuration that
    // passed the UWP PING/PONG test.
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
        return Err(io::Error::last_os_error().into());
    }

    let attributes = SecurityAttributes {
        length: std::mem::size_of::<SecurityAttributes>()
            as u32,
        descriptor,
        inherit_handle: 0,
    };

    println!("Pipe: {}", pipe_path);
    println!("Audio and IPC server running.");
    println!("Press CTRL+C to stop.");
    println!();

    loop {
        const PIPE_ACCESS_DUPLEX: u32 = 0x00000003;
        const FIRST_INSTANCE: u32 = 0x00080000;
        const REJECT_REMOTE: u32 = 0x00000008;

        let pipe = unsafe {
            CreateNamedPipeW(
                wide(&pipe_path).as_ptr(),
                PIPE_ACCESS_DUPLEX | FIRST_INSTANCE,
                REJECT_REMOTE,
                1,
                4096,
                4096,
                0,
                &attributes,
            )
        };

        if pipe as isize == INVALID_HANDLE {
            let error = io::Error::last_os_error();

            unsafe {
                LocalFree(descriptor);
            }

            return Err(error.into());
        }

        println!("Waiting for Game Bar client...");

        let result =
            serve_client(pipe, &pipeline, &mut current_gain);

        unsafe {
            CloseHandle(pipe);
        }

        if let Err(error) = result {
            eprintln!("Client error: {}", error);
        }

        println!();
    }
}
