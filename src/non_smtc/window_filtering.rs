use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetWindow, GetWindowLongPtrW, IsIconic, IsWindow, IsWindowVisible, GA_ROOT,
    GWL_EXSTYLE, GWL_STYLE, GW_OWNER, WS_CHILD, WS_EX_TOOLWINDOW, WS_VISIBLE,
};

use super::window_discovery::{WindowCandidate, WindowHandle};

#[derive(Clone, Debug)]
pub struct RankedWindow {
    pub candidate: WindowCandidate,
    pub score: i32,
}

pub fn ranked_playback_windows(candidates: Vec<WindowCandidate>) -> Vec<RankedWindow> {
    let mut ranked = candidates
        .into_iter()
        .filter(is_valid_playback_window)
        .map(|candidate| {
            let score = playback_window_score(&candidate);
            RankedWindow { candidate, score }
        })
        .collect::<Vec<_>>();

    ranked.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.candidate.z_order.cmp(&right.candidate.z_order))
    });
    ranked
}

pub fn validate_window_for_process(handle: WindowHandle, process_id: u32) -> bool {
    let hwnd = handle.hwnd();
    if unsafe { IsWindow(hwnd) }.as_bool() {
        let mut owner_process_id = 0u32;
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
                hwnd,
                Some(&mut owner_process_id),
            );
        }
        owner_process_id == process_id && base_window_filter(hwnd)
    } else {
        false
    }
}

fn is_valid_playback_window(candidate: &WindowCandidate) -> bool {
    base_window_filter(candidate.handle.hwnd())
        && !looks_like_utility_window(&candidate.title, &candidate.class_name)
        && candidate
            .rect
            .is_some_and(|rect| rect.right - rect.left >= 160 && rect.bottom - rect.top >= 90)
}

fn base_window_filter(hwnd: windows::Win32::Foundation::HWND) -> bool {
    if !unsafe { IsWindow(hwnd) }.as_bool() || !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return false;
    }

    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    if root != hwnd {
        return false;
    }

    let owner = unsafe { GetWindow(hwnd, GW_OWNER) }.unwrap_or_default();
    if !owner.0.is_null() {
        return false;
    }

    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32;
    let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;

    style & WS_VISIBLE.0 != 0
        && style & WS_CHILD.0 == 0
        && ex_style & WS_EX_TOOLWINDOW.0 == 0
        && !unsafe { IsIconic(hwnd) }.as_bool()
}

fn playback_window_score(candidate: &WindowCandidate) -> i32 {
    let title = candidate.title.to_ascii_lowercase();
    let class_name = candidate.class_name.to_ascii_lowercase();
    let mut score = 100i32 - candidate.z_order.min(40) as i32;

    if title.contains("vlc") || class_name.contains("qt") {
        score += 30;
    }
    if title.contains("mpc") || title.contains("media player classic") {
        score += 30;
    }
    if title.contains("potplayer") {
        score += 30;
    }
    if title.contains("playing") || title.contains("playlist") {
        score += 10;
    }
    if title.is_empty() {
        score -= 30;
    }
    if looks_like_utility_window(&title, &class_name) {
        score -= 100;
    }
    if let Some(rect) = candidate.rect {
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width >= 480 && height >= 270 {
            score += 20;
        }
    }

    score
}

fn looks_like_utility_window(title: &str, class_name: &str) -> bool {
    let title = title.to_ascii_lowercase();
    let class_name = class_name.to_ascii_lowercase();
    title.contains("preferences")
        || title.contains("settings")
        || title.contains("about")
        || title.contains("splash")
        || title.contains("update")
        || class_name.contains("tool")
        || class_name.contains("tooltip")
        || class_name.contains("splash")
}
