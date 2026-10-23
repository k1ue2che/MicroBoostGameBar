
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[path = "../audio.rs"]
mod audio;

use std::error::Error;
use std::ffi::{c_void, OsStr};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr::null_mut;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

const PACKAGE_FAMILY: &str =
    "0308c356-fa3a-4929-a83c-d7b7e59f5f16_yc2qef1tvgt8w";

const PIPE_NAME: &str = "MicroBoostGameBar";
const INVALID_HANDLE: isize = -1;
const ERROR_PIPE_CONNECTED: u32 = 535;

static LOG_FILE: OnceLock<Mutex<File>> = OnceLock::new();

fn initialize_log() -> io::Result<PathBuf> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| {
            io::Error::other("LOCALAPPDATA is unavailable")
        })?;

    let folder = PathBuf::from(local_app_data)
        .join("Packages")
        .join(PACKAGE_FAMILY)
        .join("LocalState");

    fs::create_dir_all(&folder)?;

    let path = folder.join("microboost-engine.log");

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;

    LOG_FILE
        .set(Mutex::new(file))
        .map_err(|_| io::Error::other("Log already initialized"))?;

    Ok(path)
}

fn log(message: impl AsRef<str>) {
    let message = message.as_ref();

    if let Some(file) = LOG_FILE.get() {
        if let Ok(mut file) = file.lock() {
            let _ = writeln!(
                file,
                "{:?} [PID {}] {}",
                SystemTime::now(),
                std::process::id(),
                message
            );

            let _ = file.flush();
        }
    }

    // Display output when a console is available.
    // Do not panic if a packaged launch has no stdout.
    let _ = writeln!(io::stdout(), "{}", message);
}

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
    // Verified SID of our current MicroBoostGameBar
    // development package.
    const VERIFIED_PACKAGE_SID: &str =
        "S-1-15-2-3839828173-443747574-3320582611-1706431060-4090298339-477087220-1129925803";

    log("Resolving AppContainer SID...");

    let mut sid: *mut c_void = null_mut();

    let result = unsafe {
        DeriveAppContainerSidFromAppContainerName(
            wide(PACKAGE_FAMILY).as_ptr(),
            &mut sid,
        )
    };

    if result != 0 {
        return Err(io::Error::other(format!(
            "DeriveAppContainerSidFromAppContainerName failed: 0x{:08X}",
            result as u32
        )));
    }

    // Some packaged full-trust processes report S_OK
    // but return a NULL SID pointer.
    // Never pass that pointer to ConvertSidToStringSidW.
    if sid.is_null() {
        log(
            "AppContainer SID API returned NULL. \
             Using verified package SID fallback."
        );

        return Ok(VERIFIED_PACKAGE_SID.to_string());
    }

    log("AppContainer SID pointer is valid.");

    let mut text: *mut u16 = null_mut();

    let converted = unsafe {
        ConvertSidToStringSidW(sid, &mut text)
    };

    if converted == 0 {
        let error = io::Error::last_os_error();

        unsafe {
            FreeSid(sid);
        }

        return Err(io::Error::other(format!(
            "ConvertSidToStringSidW failed: {}",
            error
        )));
    }

    let mut length = 0usize;

    unsafe {
        while *text.add(length) != 0 {
            length += 1;
        }
    }

    let value = unsafe {
        String::from_utf16_lossy(
            std::slice::from_raw_parts(text, length)
        )
    };

    unsafe {
        LocalFree(text.cast());
        FreeSid(sid);
    }

    log(format!("AppContainer SID resolved: {}", value));

    Ok(value)
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

                log(format!("Boost updated: {}%", percent));

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

    log("Client connected");

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

    log(format!("Command: {}", command.trim()));

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
            "Incomplete pipe response"
        ));
    }

    log(format!("Response: {}", response.trim()));

    Ok(())
}

fn run() -> Result<(), Box<dyn Error>> {
    log("STEP 1: Engine process started");

    log(format!(
        "Executable: {:?}",
        std::env::current_exe()?
    ));

    log(format!(
        "Working directory: {:?}",
        std::env::current_dir()?
    ));

    log("STEP 2: Initializing audio pipeline");

    // The previously tested audio implementation.
    let pipeline = audio::AudioPipeline::start()?;

    log("STEP 3: Audio pipeline initialized");
    log("Audio gain: 100%");
    log("Audio output: VB-CABLE");

    let mut current_gain = 100u32;

    log("STEP 4: Resolving AppContainer package SID");

    let sid = package_sid()?;

    log("Resolving Windows session ID...");

    let session = session_id()?;

    log(format!("Windows session resolved: {}", session));

    log(format!("Windows session: {}", session));
    log(format!("Package SID: {}", sid));

    let pipe_path = format!(
        r"\\?\pipe\Sessions\{}\AppContainerNamedObjects\{}\{}",
        session,
        sid,
        PIPE_NAME
    );

    log(format!("Named pipe path: {}", pipe_path));

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

    log("STEP 5: IPC security descriptor initialized");

    const PIPE_ACCESS_DUPLEX: u32 = 0x00000003;
    const FIRST_INSTANCE: u32 = 0x00080000;
    const REJECT_REMOTE: u32 = 0x00000008;

    loop {
        log("STEP 6: Creating named pipe");

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

            log(format!(
                "ERROR: CreateNamedPipeW failed: {}",
                error
            ));

            return Err(error.into());
        }

        log("STEP 7: Named pipe ready");
        log("Waiting for Game Bar client");

        let result =
            serve_client(pipe, &pipeline, &mut current_gain);

        unsafe {
            CloseHandle(pipe);
        }

        if let Err(error) = result {
            log(format!("Client error: {}", error));
        }
    }
}

fn main() {
    // Initialize diagnostics before any audio or pipe work.
    match initialize_log() {
        Ok(path) => {
            log("=================================");
            log("MICROBOOST ENGINE STARTUP");
            log(format!("Log file: {}", path.display()));
        }
        Err(error) => {
            let _ = writeln!(
                io::stderr(),
                "Cannot initialize startup log: {}",
                error
            );
        }
    }

    std::panic::set_hook(Box::new(|info| {
        log(format!("RUST PANIC: {}", info));
    }));

    match run() {
        Ok(()) => {
            log("Engine exited normally");
        }
        Err(error) => {
            log(format!("FATAL ERROR: {}", error));
            std::process::exit(1);
        }
    }
}
