/// Toggle the default audio capture (microphone) mute state.
/// Platform dispatch: Windows uses the Core Audio COM API (no third-party
/// dependencies); Linux goes through `platform::mic` (pactl/wpctl, SPEC §4).
/// Profile-agnostic: called directly from the input loop on any profile.
use std::sync::atomic::AtomicBool;

/// Cached mute state — written by toggle_mute() and init(), read by the output loop.
pub static MIC_MUTED: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
mod inner {
    use super::MIC_MUTED;
    use std::sync::atomic::Ordering;

    use windows::Win32::Foundation::BOOL;
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eConsole,
    };
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    };

    pub fn init() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if let Some(muted) = query_muted() {
                MIC_MUTED.store(muted, Ordering::Relaxed);
                log::debug!(
                    "mic: initial state = {}",
                    if muted { "muted" } else { "unmuted" }
                );
            }
        }
    }

    fn query_muted() -> Option<bool> {
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            let device = enumerator
                .GetDefaultAudioEndpoint(eCapture, eConsole)
                .ok()?;
            let vol: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None).ok()?;
            Some(vol.GetMute().ok()?.as_bool())
        }
    }

    pub fn toggle_mute() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

            let Ok(enumerator): Result<IMMDeviceEnumerator, _> =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
            else {
                log::warn!("mic: CoCreateInstance(MMDeviceEnumerator) failed");
                return;
            };

            let Ok(device) = enumerator.GetDefaultAudioEndpoint(eCapture, eConsole) else {
                log::warn!("mic: no default microphone found");
                return;
            };

            let Ok(vol): Result<IAudioEndpointVolume, _> = device.Activate(CLSCTX_ALL, None) else {
                log::warn!("mic: Activate(IAudioEndpointVolume) failed");
                return;
            };

            let muted = vol.GetMute().unwrap_or(BOOL(0)).as_bool();
            if let Err(e) = vol.SetMute(!muted, std::ptr::null()) {
                log::warn!("mic: SetMute failed: {e}");
                return;
            }

            let new_state = !muted;
            MIC_MUTED.store(new_state, Ordering::Relaxed);
            log::info!("mic: {}", if new_state { "muted" } else { "unmuted" });
        }
    }
}

/// Linux: query the real mute state via pactl/wpctl and prime MIC_MUTED.
/// Best-effort — if neither tool is available the cache keeps its default.
#[cfg(target_os = "linux")]
fn linux_init() {
    use std::sync::atomic::Ordering;
    if let Some(muted) = crate::platform::mic_is_muted() {
        MIC_MUTED.store(muted, Ordering::Relaxed);
        log::debug!(
            "mic: initial state = {}",
            if muted { "muted" } else { "unmuted" }
        );
    }
}

/// Linux: toggle via pactl/wpctl, then re-query the real state so the LED
/// mirrors the system (Windows behavior). If the query fails, fall back to
/// the cached toggle result so the LED still tracks the toggle.
#[cfg(target_os = "linux")]
fn linux_toggle_mute() {
    use std::sync::atomic::Ordering;
    match crate::platform::mic_toggle() {
        Ok(()) => {
            let new_state = crate::platform::mic_is_muted()
                .unwrap_or_else(|| !MIC_MUTED.load(Ordering::Relaxed));
            MIC_MUTED.store(new_state, Ordering::Relaxed);
            log::info!("mic: {}", if new_state { "muted" } else { "unmuted" });
        }
        Err(e) => {
            log::warn!("mic: toggle failed: {e}");
        }
    }
}

/// Query the current system mute state and prime MIC_MUTED.
/// Call once at startup (on a blocking thread) before the first output frame.
pub fn init() {
    #[cfg(windows)]
    inner::init();
    #[cfg(target_os = "linux")]
    linux_init();
}

/// Toggle system mic mute and update MIC_MUTED.
pub fn toggle_mute() {
    #[cfg(windows)]
    inner::toggle_mute();
    #[cfg(target_os = "linux")]
    linux_toggle_mute();
}
