use std::{mem::size_of, path::PathBuf};

use windows::{
    core::PWSTR,
    Win32::{
        Foundation::{CloseHandle, ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, HANDLE},
        Storage::Packaging::Appx::GetPackageFullName,
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
    },
};

use crate::{
    media_source::{
        normalize_component, BrowserFamily, MediaSource, MediaSourceId, MediaSourceKind,
        ProcessIdentity,
    },
    smtc::SmtcSessionKey,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessSnapshot {
    pub process_id: u32,
    pub executable_path: Option<PathBuf>,
    pub executable_name: String,
    pub package_full_name: Option<String>,
}

impl ProcessSnapshot {
    fn identity(&self) -> ProcessIdentity {
        ProcessIdentity {
            process_id: self.process_id,
            executable_path: self.executable_path.clone(),
            executable_name: self.executable_name.clone(),
            package_full_name: self.package_full_name.clone(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ProcessResolver;

impl ProcessResolver {
    pub fn resolve_media_source(&self, source_app_user_model_id: &str) -> MediaSource {
        let processes = enumerate_processes();
        let normalized_aumid = source_app_user_model_id.to_ascii_lowercase();

        let matched = processes
            .iter()
            .filter(|process| process_matches_aumid(process, &normalized_aumid))
            .min_by_key(|process| process_rank(process, &normalized_aumid))
            .cloned();

        match matched {
            Some(process) => media_source_from_process(source_app_user_model_id, process),
            None => MediaSource::unresolved(source_app_user_model_id.to_string()),
        }
    }

    pub fn resolve_session_key(&self, source_app_user_model_id: &str) -> SmtcSessionKey {
        let source = self.resolve_media_source(source_app_user_model_id);
        SmtcSessionKey::from_source(&source)
    }
}

fn media_source_from_process(
    source_app_user_model_id: &str,
    process: ProcessSnapshot,
) -> MediaSource {
    let browser_family = browser_family_for_exe(&process.executable_name);
    let kind = match browser_family.clone() {
        Some(family) => MediaSourceKind::Browser(family),
        None if process.package_full_name.is_some() => MediaSourceKind::StoreApp,
        None => MediaSourceKind::DesktopApp,
    };

    let id = match &kind {
        MediaSourceKind::Browser(family) => MediaSourceId::new(format!(
            "browser:{family}:{}",
            browser_profile_key(source_app_user_model_id)
        )),
        MediaSourceKind::StoreApp => MediaSourceId::new(format!(
            "store:{}",
            process
                .package_full_name
                .as_deref()
                .map(normalize_component)
                .unwrap_or_else(|| normalize_component(source_app_user_model_id))
        )),
        MediaSourceKind::DesktopApp | MediaSourceKind::Unknown => MediaSourceId::new(format!(
            "process:{}",
            process
                .executable_path
                .as_ref()
                .map(|path| normalize_component(&path.to_string_lossy()))
                .unwrap_or_else(|| normalize_component(&process.executable_name))
        )),
    };

    MediaSource {
        id,
        kind,
        source_app_user_model_id: source_app_user_model_id.to_string(),
        process: Some(process.identity()),
    }
}

fn enumerate_processes() -> Vec<ProcessSnapshot> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    let Ok(snapshot) = snapshot else {
        tracing::warn!("failed to create process snapshot for SMTC resolution");
        return Vec::new();
    };

    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut processes = Vec::new();

    let first = unsafe { Process32FirstW(snapshot, &mut entry) };
    if first.is_ok() {
        loop {
            let process_id = entry.th32ProcessID;
            let fallback_name = fixed_utf16_to_string(&entry.szExeFile);
            processes.push(resolve_process(process_id, fallback_name));

            if unsafe { Process32NextW(snapshot, &mut entry) }.is_err() {
                break;
            }
        }
    }

    unsafe {
        let _ = CloseHandle(snapshot);
    }
    processes
}

fn resolve_process(process_id: u32, fallback_name: String) -> ProcessSnapshot {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) };
    let Ok(handle) = handle else {
        return ProcessSnapshot {
            process_id,
            executable_path: None,
            executable_name: fallback_name,
            package_full_name: None,
        };
    };

    let executable_path = query_process_image_path(handle);
    let package_full_name = query_package_full_name(handle);
    unsafe {
        let _ = CloseHandle(handle);
    }

    let executable_name = executable_path
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or(fallback_name);

    ProcessSnapshot {
        process_id,
        executable_path,
        executable_name,
        package_full_name,
    }
}

fn query_process_image_path(handle: HANDLE) -> Option<PathBuf> {
    let mut buffer = vec![0u16; 32_768];
    let mut length = buffer.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    };

    result
        .ok()
        .map(|_| PathBuf::from(String::from_utf16_lossy(&buffer[..length as usize])))
}

fn query_package_full_name(handle: HANDLE) -> Option<String> {
    let mut length = 0u32;
    let first = unsafe { GetPackageFullName(handle, &mut length, PWSTR::null()) };
    if first != ERROR_INSUFFICIENT_BUFFER || length == 0 {
        return None;
    }

    let mut buffer = vec![0u16; length as usize];
    let second = unsafe { GetPackageFullName(handle, &mut length, PWSTR(buffer.as_mut_ptr())) };
    if second != ERROR_SUCCESS {
        return None;
    }

    let text = String::from_utf16_lossy(&buffer[..length.saturating_sub(1) as usize]);
    (!text.is_empty()).then_some(text)
}

fn process_matches_aumid(process: &ProcessSnapshot, normalized_aumid: &str) -> bool {
    let exe = process.executable_name.to_ascii_lowercase();
    let package = process
        .package_full_name
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let path = process
        .executable_path
        .as_ref()
        .map(|path| path.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    normalized_aumid.contains(exe.trim_end_matches(".exe"))
        || (!package.is_empty() && normalized_aumid.contains(&package))
        || known_aumid_exe_match(normalized_aumid, &exe)
        || browser_family_for_exe(&exe).is_some() && browser_aumid(normalized_aumid)
        || (!path.is_empty() && normalized_aumid.contains(&exe))
}

fn process_rank(process: &ProcessSnapshot, normalized_aumid: &str) -> u8 {
    let exe = process.executable_name.to_ascii_lowercase();
    if known_aumid_exe_match(normalized_aumid, &exe) {
        0
    } else if browser_family_for_exe(&exe).is_some() && browser_aumid(normalized_aumid) {
        1
    } else if process.package_full_name.is_some() {
        2
    } else {
        3
    }
}

fn known_aumid_exe_match(normalized_aumid: &str, exe: &str) -> bool {
    matches!(
        (normalized_aumid, exe),
        (aumid, "spotify.exe") if aumid.contains("spotify")
    ) || matches!(
        (normalized_aumid, exe),
        (aumid, "msedge.exe") if aumid.contains("microsoftedge") || aumid.contains("microsoft.edge") || aumid.contains("edge")
    ) || matches!(
        (normalized_aumid, exe),
        (aumid, "chrome.exe") if aumid.contains("chrome") || aumid.contains("youtube")
    ) || matches!(
        (normalized_aumid, exe),
        (aumid, "netflix.exe") if aumid.contains("netflix")
    )
}

fn browser_aumid(normalized_aumid: &str) -> bool {
    normalized_aumid.contains("chrome")
        || normalized_aumid.contains("edge")
        || normalized_aumid.contains("youtube")
        || normalized_aumid.contains("brave")
        || normalized_aumid.contains("firefox")
}

fn browser_family_for_exe(executable_name: &str) -> Option<BrowserFamily> {
    match executable_name.to_ascii_lowercase().as_str() {
        "chrome.exe" => Some(BrowserFamily::Chrome),
        "msedge.exe" => Some(BrowserFamily::Edge),
        "brave.exe" => Some(BrowserFamily::Brave),
        "firefox.exe" => Some(BrowserFamily::Firefox),
        _ => None,
    }
}

fn browser_profile_key(source_app_user_model_id: &str) -> String {
    let normalized = normalize_component(source_app_user_model_id);
    if normalized.contains("youtube") {
        "youtube_music".to_string()
    } else {
        "default".to_string()
    }
}

fn fixed_utf16_to_string(buffer: &[u16]) -> String {
    let length = buffer
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..length])
}
