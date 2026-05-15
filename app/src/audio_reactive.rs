//! 音乐响应表演的数据源与 Tauri 命令。
//!
//! 本模块只负责启动/停止音乐响应会话，并把 fake timer 或 WASAPI loopback 分析成统一的 `MusicDanceFrame`。
//! 它不理解 sprite、窗口移动或舞蹈编排，前端 `MusicReactivePlayer` 通过 `performance-frame` 消费这些帧。
//! fake 源用于先调表演体验，WASAPI 源用于捕获电脑正在播放的真实声音。

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tracing::{debug, info, warn};

/// 音乐响应运行状态。
#[derive(Default)]
pub struct SharedAudioReactive {
    current: Mutex<Option<AudioReactiveRun>>,
}

struct AudioReactiveRun {
    session_id: u64,
    stop: Arc<AtomicBool>,
}

/// 发给前端音乐响应播放器的单帧分析数据。
#[derive(Debug, Clone, Copy, Serialize)]
pub struct MusicDanceFrame {
    pub session_id: u64,
    pub energy: f32,
    pub bass: f32,
    pub onset: bool,
    pub silence: bool,
}

const FAKE_FRAME_MS: u64 = 100;
const WASAPI_POLL_MS: u64 = 80;

#[tauri::command]
pub fn cmd_start_fake_music_dance(
    app: AppHandle,
    shared: tauri::State<'_, SharedAudioReactive>,
) -> Result<u64, String> {
    start_music_source(app, &shared, "fake")
}

#[tauri::command]
pub fn cmd_start_wasapi_music_dance(
    app: AppHandle,
    shared: tauri::State<'_, SharedAudioReactive>,
) -> Result<u64, String> {
    start_music_source(app, &shared, "wasapi")
}

#[tauri::command]
pub fn cmd_stop_music_dance(
    app: AppHandle,
    shared: tauri::State<'_, SharedAudioReactive>,
) -> Result<(), String> {
    stop_music_dance(&app, &shared, "user_stop");
    Ok(())
}

/// 停止当前音乐响应数据源，并通知前端结束对应表演。
pub fn stop_music_dance(app: &AppHandle, shared: &SharedAudioReactive, reason: &'static str) {
    stop_current(shared, Some(app), reason);
}

fn start_music_source(
    app: AppHandle,
    shared: &tauri::State<'_, SharedAudioReactive>,
    source: &'static str,
) -> Result<u64, String> {
    stop_current(shared.inner(), Some(&app), "replaced");

    let session = ai_pad_core::performance::start_performance(
        ai_pad_core::performance::PerformanceKind::MusicReactiveDance,
    );
    let payload = serde_json::json!({
        "session_id": session.id,
        "kind": "music-reactive",
        "source": source,
    });
    app.emit("performance-start", &payload)
        .map_err(|e| format!("emit performance-start failed: {e}"))?;
    ai_pad_core::performance::update_phase(
        session.id,
        ai_pad_core::performance::PerformancePhase::Active,
    );

    let stop = Arc::new(AtomicBool::new(false));
    *shared.current.lock().map_err(|e| e.to_string())? = Some(AudioReactiveRun {
        session_id: session.id,
        stop: Arc::clone(&stop),
    });

    match source {
        "fake" => spawn_fake_music_loop(app, session.id, stop),
        "wasapi" => spawn_wasapi_music_loop(app, session.id, stop),
        _ => unreachable!(),
    }

    info!(session_id = session.id, source, "[audio-reactive] started");
    Ok(session.id)
}

fn stop_current(shared: &SharedAudioReactive, app: Option<&AppHandle>, reason: &'static str) {
    let Ok(mut guard) = shared.current.lock() else {
        return;
    };
    if let Some(run) = guard.take() {
        run.stop.store(true, Ordering::Relaxed);
        ai_pad_core::performance::stop_performance(run.session_id, reason);
        if let Some(app) = app {
            emit_stop(app, run.session_id, reason);
        }
    }
}

fn finish_source(app: &AppHandle, session_id: u64, reason: &str) {
    emit_stop(app, session_id, reason);
    ai_pad_core::performance::stop_performance(session_id, reason);
}

fn emit_stop(app: &AppHandle, session_id: u64, reason: &str) {
    let _ = app.emit(
        "performance-stop",
        serde_json::json!({
            "session_id": session_id,
            "reason": reason,
        }),
    );
}

fn emit_frame(app: &AppHandle, frame: MusicDanceFrame) {
    if let Err(e) = app.emit("performance-frame", frame) {
        debug!(error = %e, session_id = frame.session_id, "[audio-reactive] emit frame failed");
    }
}

fn spawn_fake_music_loop(app: AppHandle, session_id: u64, stop: Arc<AtomicBool>) {
    tauri::async_runtime::spawn(async move {
        let start = Instant::now();
        let mut tick = tokio::time::interval(Duration::from_millis(FAKE_FRAME_MS));
        let mut beat = 0u64;
        loop {
            tick.tick().await;
            if stop.load(Ordering::Relaxed) {
                break;
            }
            if !ai_pad_core::performance::is_performing() {
                break;
            }
            if crate::shutdown::is_requested() {
                break;
            }

            beat += 1;
            let t = start.elapsed().as_secs_f32();
            let pulse = ((t * std::f32::consts::TAU * 1.7).sin() + 1.0) * 0.5;
            let bass_pulse = ((t * std::f32::consts::TAU * 0.85).sin() + 1.0) * 0.5;
            let onset = beat.is_multiple_of(9) || (bass_pulse > 0.94 && beat.is_multiple_of(3));
            let silence = (t as u32 % 17) == 13;

            emit_frame(
                &app,
                MusicDanceFrame {
                    session_id,
                    energy: if silence { 0.0 } else { 0.18 + pulse * 0.72 },
                    bass: if silence {
                        0.0
                    } else {
                        0.12 + bass_pulse * 0.86
                    },
                    onset,
                    silence,
                },
            );
        }
    });
}

fn spawn_wasapi_music_loop(app: AppHandle, session_id: u64, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let result = wasapi_loopback(session_id, &stop, |frame| emit_frame(&app, frame));
        match result {
            Ok(()) => finish_source(&app, session_id, "stopped"),
            Err(e) => {
                warn!(error = %e, session_id, "[audio-reactive] WASAPI failed");
                let _ = app.emit(
                    "performance-error",
                    serde_json::json!({
                        "session_id": session_id,
                        "message": e,
                        "recoverable": true,
                    }),
                );
                ai_pad_core::performance::fail_performance(session_id, e);
            }
        }
    });
}

#[derive(Debug, Clone, Copy)]
struct EnergyStats {
    energy: f32,
    bass: f32,
    silence: bool,
}

fn analyze_samples(samples: &[f32], channels: usize) -> EnergyStats {
    if samples.is_empty() || channels == 0 {
        return EnergyStats {
            energy: 0.0,
            bass: 0.0,
            silence: true,
        };
    }

    let mut sum_sq = 0.0f32;
    let mut bass_sum = 0.0f32;
    let mut bass_count = 0usize;
    let frame_count = samples.len() / channels;

    for frame in 0..frame_count {
        let base = frame * channels;
        let mut mono = 0.0f32;
        for ch in 0..channels {
            mono += samples[base + ch];
        }
        mono /= channels as f32;
        sum_sq += mono * mono;

        if frame % 12 == 0 {
            bass_sum += mono.abs();
            bass_count += 1;
        }
    }

    let rms = (sum_sq / frame_count.max(1) as f32).sqrt();
    let bass_raw = if bass_count == 0 {
        0.0
    } else {
        bass_sum / bass_count as f32
    };
    let energy = (rms * 5.0).clamp(0.0, 1.0);
    let bass = (bass_raw * 4.5).clamp(0.0, 1.0);

    EnergyStats {
        energy,
        bass,
        silence: rms < 0.008,
    }
}

#[cfg(windows)]
fn wasapi_loopback(
    session_id: u64,
    stop: &AtomicBool,
    mut on_frame: impl FnMut(MusicDanceFrame),
) -> Result<(), String> {
    use std::slice;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator,
        MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_LOOPBACK,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED,
    };

    struct ComGuard;
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }

    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() }
        .map_err(|e| format!("CoInitializeEx failed: {e}"))?;
    let _com_guard = ComGuard;

    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
            .map_err(|e| format!("CoCreateInstance IMMDeviceEnumerator failed: {e}"))?;
    let device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }
        .map_err(|e| format!("GetDefaultAudioEndpoint failed: {e}"))?;
    let audio_client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }
        .map_err(|e| format!("Activate IAudioClient failed: {e}"))?;

    let format_ptr =
        unsafe { audio_client.GetMixFormat() }.map_err(|e| format!("GetMixFormat failed: {e}"))?;
    if format_ptr.is_null() {
        return Err("GetMixFormat returned null".into());
    }

    let format = unsafe { *format_ptr };
    let channels = format.nChannels.max(1) as usize;
    let block_align = format.nBlockAlign.max(1) as usize;
    let bits_per_sample = format.wBitsPerSample;
    let sample_format = SampleFormat::from_wave_format(format_ptr, &format)?;

    unsafe {
        audio_client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK,
            500_000,
            0,
            format_ptr,
            None,
        )
    }
    .map_err(|e| {
        unsafe { CoTaskMemFree(Some(format_ptr as *const _)) };
        format!("IAudioClient Initialize failed: {e}")
    })?;
    unsafe { CoTaskMemFree(Some(format_ptr as *const _)) };

    let capture: IAudioCaptureClient = unsafe { audio_client.GetService() }
        .map_err(|e| format!("GetService IAudioCaptureClient failed: {e}"))?;
    unsafe { audio_client.Start() }.map_err(|e| format!("IAudioClient Start failed: {e}"))?;

    let mut previous_energy = 0.0f32;
    let mut frame_bucket = EnergyStats {
        energy: 0.0,
        bass: 0.0,
        silence: true,
    };
    let mut has_bucket = false;
    let mut bucket_onset = false;
    while !stop.load(Ordering::Relaxed) {
        if !ai_pad_core::performance::is_performing() || crate::shutdown::is_requested() {
            break;
        }
        std::thread::sleep(Duration::from_millis(WASAPI_POLL_MS));
        let mut packet = unsafe { capture.GetNextPacketSize() }
            .map_err(|e| format!("GetNextPacketSize failed: {e}"))?;

        while packet > 0 {
            let mut data = std::ptr::null_mut();
            let mut frames = 0u32;
            let mut flags = 0u32;
            unsafe { capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None) }
                .map_err(|e| format!("GetBuffer failed: {e}"))?;

            let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;
            let samples = if silent || data.is_null() {
                Vec::new()
            } else {
                let bytes = unsafe {
                    slice::from_raw_parts(data as *const u8, frames as usize * block_align)
                };
                decode_samples(bytes, sample_format, bits_per_sample, channels)
            };
            unsafe { capture.ReleaseBuffer(frames) }
                .map_err(|e| format!("ReleaseBuffer failed: {e}"))?;

            let stats = analyze_samples(&samples, channels);
            let onset = !stats.silence && stats.energy > previous_energy + 0.16;
            previous_energy = previous_energy * 0.82 + stats.energy * 0.18;

            frame_bucket.energy = frame_bucket.energy.max(stats.energy);
            frame_bucket.bass = frame_bucket.bass.max(stats.bass);
            frame_bucket.silence = frame_bucket.silence && (silent || stats.silence);
            has_bucket = true;
            bucket_onset = bucket_onset || onset;

            packet = unsafe { capture.GetNextPacketSize() }
                .map_err(|e| format!("GetNextPacketSize failed: {e}"))?;
        }

        if has_bucket {
            on_frame(MusicDanceFrame {
                session_id,
                energy: frame_bucket.energy,
                bass: frame_bucket.bass,
                onset: bucket_onset,
                silence: frame_bucket.silence,
            });
            frame_bucket = EnergyStats {
                energy: 0.0,
                bass: 0.0,
                silence: true,
            };
            has_bucket = false;
            bucket_onset = false;
        }
    }

    let _ = unsafe { audio_client.Stop() };
    Ok(())
}

#[cfg(not(windows))]
fn wasapi_loopback(
    _session_id: u64,
    _stop: &AtomicBool,
    _on_frame: impl FnMut(MusicDanceFrame),
) -> Result<(), String> {
    Err("WASAPI loopback is only available on Windows".into())
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy)]
enum SampleFormat {
    F32,
    I16,
    U8,
    I32,
}

#[cfg(windows)]
impl SampleFormat {
    fn from_wave_format(
        ptr: *const windows::Win32::Media::Audio::WAVEFORMATEX,
        format: &windows::Win32::Media::Audio::WAVEFORMATEX,
    ) -> Result<Self, String> {
        use windows::Win32::Media::Audio::WAVE_FORMAT_PCM;
        use windows::Win32::Media::KernelStreaming::{
            KSDATAFORMAT_SUBTYPE_PCM, WAVE_FORMAT_EXTENSIBLE,
        };
        use windows::Win32::Media::Multimedia::{
            KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, WAVE_FORMAT_IEEE_FLOAT,
        };

        match (format.wFormatTag as u32, format.wBitsPerSample) {
            (WAVE_FORMAT_PCM, 8) => Ok(Self::U8),
            (WAVE_FORMAT_PCM, 16) => Ok(Self::I16),
            (WAVE_FORMAT_PCM, 32) => Ok(Self::I32),
            (WAVE_FORMAT_IEEE_FLOAT, 32) => Ok(Self::F32),
            (WAVE_FORMAT_EXTENSIBLE, _) => {
                let sub_format = unsafe {
                    let ext = ptr as *const windows::Win32::Media::Audio::WAVEFORMATEXTENSIBLE;
                    let sub_ptr = std::ptr::addr_of!((*ext).SubFormat);
                    std::ptr::read_unaligned(sub_ptr)
                };
                if sub_format == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT {
                    Ok(Self::F32)
                } else if sub_format == KSDATAFORMAT_SUBTYPE_PCM {
                    match format.wBitsPerSample {
                        8 => Ok(Self::U8),
                        16 => Ok(Self::I16),
                        32 => Ok(Self::I32),
                        other => Err(format!("unsupported PCM bits: {other}")),
                    }
                } else {
                    Err("unsupported WAVEFORMATEXTENSIBLE subtype".into())
                }
            }
            (tag, bits) => Err(format!("unsupported mix format tag={tag} bits={bits}")),
        }
    }
}

#[cfg(windows)]
fn decode_samples(bytes: &[u8], format: SampleFormat, _bits: u16, channels: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(bytes.len() / 2);
    match format {
        SampleFormat::F32 => {
            for chunk in bytes.chunks_exact(4) {
                out.push(f32::from_ne_bytes(chunk.try_into().unwrap()).clamp(-1.0, 1.0));
            }
        }
        SampleFormat::I16 => {
            for chunk in bytes.chunks_exact(2) {
                out.push(i16::from_ne_bytes(chunk.try_into().unwrap()) as f32 / i16::MAX as f32);
            }
        }
        SampleFormat::U8 => {
            for byte in bytes {
                out.push((*byte as f32 - 128.0) / 128.0);
            }
        }
        SampleFormat::I32 => {
            for chunk in bytes.chunks_exact(4) {
                out.push(i32::from_ne_bytes(chunk.try_into().unwrap()) as f32 / i32::MAX as f32);
            }
        }
    }

    if let Some(whole_len) = out
        .len()
        .checked_div(channels)
        .map(|frames| frames * channels)
    {
        out.truncate(whole_len);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_samples_detects_silence() {
        let stats = analyze_samples(&[0.0; 64], 2);
        assert!(stats.silence);
        assert_eq!(stats.energy, 0.0);
    }

    #[test]
    fn analyze_samples_normalizes_energy() {
        let samples = [0.8, 0.8, -0.8, -0.8, 0.4, 0.4, -0.4, -0.4];
        let stats = analyze_samples(&samples, 2);
        assert!(!stats.silence);
        assert!(stats.energy > 0.5);
        assert!(stats.bass > 0.0);
    }
}
