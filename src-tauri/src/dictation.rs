use crate::models::DictationConfig;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static DICTATION_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

#[cfg(not(target_os = "windows"))]
use std::process::{Child, Command, Stdio};

pub struct RecorderState {
    #[cfg(not(target_os = "windows"))]
    child: Option<Child>,
    #[cfg(target_os = "windows")]
    stop_flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    #[cfg(target_os = "windows")]
    join_handle: Option<std::thread::JoinHandle<Result<(), String>>>,
    wav_path: Option<PathBuf>,
}

impl Default for RecorderState {
    fn default() -> Self {
        Self {
            #[cfg(not(target_os = "windows"))]
            child: None,
            #[cfg(target_os = "windows")]
            stop_flag: None,
            #[cfg(target_os = "windows")]
            join_handle: None,
            wav_path: None,
        }
    }
}

pub type SharedRecorder = Mutex<RecorderState>;

#[cfg(not(target_os = "windows"))]
pub fn start_recording(state: &SharedRecorder) -> Result<(), String> {
    let mut guard = state.lock().map_err(|_| "Kayıt kilidi alınamadı".to_string())?;
    if guard.child.is_some() {
        return Ok(());
    }
    let wav_path = std::env::temp_dir().join(format!(
        "app-fabrikasi-dikte-{}.wav",
        chrono::Utc::now().timestamp_millis()
    ));

    // Try PipeWire -> PulseAudio -> ALSA (zero native C dev dependencies on Linux)
    let path_str = wav_path.to_string_lossy().to_string();
    let candidates: Vec<(&str, Vec<&str>)> = vec![
        (
            "pw-record",
            vec!["--format", "s16", "--rate", "16000", "--channels", "1", &path_str],
        ),
        (
            "parecord",
            vec![
                "--file-format=wav",
                "--format=s16le",
                "--rate=16000",
                "--channels=1",
                &path_str,
            ],
        ),
        (
            "arecord",
            vec!["-f", "S16_LE", "-r", "16000", "-c", "1", "-t", "wav", &path_str],
        ),
    ];

    let mut spawned = None;
    for (bin, args) in candidates {
        if let Ok(child) = Command::new(bin)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            spawned = Some(child);
            break;
        }
    }

    match spawned {
        Some(child) => {
            guard.child = Some(child);
            guard.wav_path = Some(wav_path);
            Ok(())
        }
        None => Err(
            "Mikrofon kayıt aracı bulunamadı (pw-record, parecord veya arecord gerekli)."
                .to_string(),
        ),
    }
}

#[cfg(target_os = "windows")]
pub fn start_recording(state: &SharedRecorder) -> Result<(), String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::io::Write;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let mut guard = state.lock().map_err(|_| "Kayıt kilidi alınamadı".to_string())?;
    if guard.stop_flag.is_some() {
        return Ok(());
    }
    let wav_path = std::env::temp_dir().join(format!(
        "app-fabrikasi-dikte-{}.wav",
        chrono::Utc::now().timestamp_millis()
    ));
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_clone = stop_flag.clone();
    let wav_clone = wav_path.clone();

    let handle = std::thread::spawn(move || -> Result<(), String> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| "Varsayılan mikrofon bulunamadı.".to_string())?;
        let config = device
            .default_input_config()
            .map_err(|e| format!("Mikrofon yapılandırması okunamadı: {e}"))?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels();
        let samples = Arc::new(Mutex::new(Vec::<i16>::new()));
        let samples_cb = samples.clone();

        let err_fn = |err| eprintln!("Mikrofon hatası: {err}");
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |data: &[f32], _| {
                    if let Ok(mut buf) = samples_cb.lock() {
                        for chunk in data.chunks(channels as usize) {
                            let mono = chunk.iter().copied().sum::<f32>() / channels as f32;
                            buf.push((mono.clamp(-1.0, 1.0) * 32767.0) as i16);
                        }
                    }
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config.into(),
                move |data: &[i16], _| {
                    if let Ok(mut buf) = samples_cb.lock() {
                        for chunk in data.chunks(channels as usize) {
                            let mono = chunk.iter().map(|&s| s as i32).sum::<i32>() / channels as i32;
                            buf.push(mono as i16);
                        }
                    }
                },
                err_fn,
                None,
            ),
            _ => return Err("Desteklenmeyen mikrofon örnek formatı.".to_string()),
        }
        .map_err(|e| format!("Mikrofon akışı başlatılamadı: {e}"))?;

        stream
            .play()
            .map_err(|e| format!("Mikrofon kaydı başlatılamadı: {e}"))?;
        while !stop_clone.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
        drop(stream);

        let pcm = samples.lock().map_err(|_| "Buffer kilidi".to_string())?.clone();
        let data_bytes = (pcm.len() * 2) as u32;
        let mut f = std::fs::File::create(&wav_clone).map_err(|e| e.to_string())?;
        f.write_all(b"RIFF").map_err(|e| e.to_string())?;
        f.write_all(&(36 + data_bytes).to_le_bytes()).map_err(|e| e.to_string())?;
        f.write_all(b"WAVEfmt ").map_err(|e| e.to_string())?;
        f.write_all(&16u32.to_le_bytes()).map_err(|e| e.to_string())?;
        f.write_all(&1u16.to_le_bytes()).map_err(|e| e.to_string())?;
        f.write_all(&1u16.to_le_bytes()).map_err(|e| e.to_string())?;
        f.write_all(&sample_rate.to_le_bytes()).map_err(|e| e.to_string())?;
        f.write_all(&(sample_rate * 2).to_le_bytes()).map_err(|e| e.to_string())?;
        f.write_all(&2u16.to_le_bytes()).map_err(|e| e.to_string())?;
        f.write_all(&16u16.to_le_bytes()).map_err(|e| e.to_string())?;
        f.write_all(b"data").map_err(|e| e.to_string())?;
        f.write_all(&data_bytes.to_le_bytes()).map_err(|e| e.to_string())?;
        for s in pcm {
            f.write_all(&s.to_le_bytes()).map_err(|e| e.to_string())?;
        }
        Ok(())
    });

    guard.stop_flag = Some(stop_flag);
    guard.join_handle = Some(handle);
    guard.wav_path = Some(wav_path);
    Ok(())
}

pub async fn stop_and_transcribe(
    state: &SharedRecorder,
    cfg: &DictationConfig,
) -> Result<String, String> {
    let wav_path = {
        let mut guard = state.lock().map_err(|_| "Kayıt kilidi alınamadı".to_string())?;

        #[cfg(not(target_os = "windows"))]
        if let Some(mut child) = guard.child.take() {
            // Send SIGINT so pw-record/arecord flushes WAV header cleanly
            let pid = child.id().to_string();
            let _ = Command::new("kill").args(["-INT", &pid]).status();
            let mut waited = false;
            for _ in 0..20 {
                if let Ok(Some(_)) = child.try_wait() {
                    waited = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
            if !waited {
                let _ = child.kill();
                let _ = child.wait();
            }
        }

        #[cfg(target_os = "windows")]
        {
            if let Some(flag) = guard.stop_flag.take() {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            if let Some(handle) = guard.join_handle.take() {
                let _ = handle.join();
            }
        }

        guard
            .wav_path
            .take()
            .ok_or_else(|| "Aktif ses kaydı bulunamadı.".to_string())?
    };

    let wav_bytes = std::fs::read(&wav_path).map_err(|e| format!("Ses dosyası okunamadı: {e}"))?;
    let _ = std::fs::remove_file(&wav_path);

    if wav_bytes.len() < 1200 {
        return Err("Ses kaydı çok kısa, lütfen tekrar deneyin.".to_string());
    }

    if cfg.api_key.trim().is_empty() {
        return Err(
            "Dikte API anahtarı ayarlanmamış. Ayarlar → Dikte bölümünden Groq veya OpenAI anahtarınızı girin."
                .to_string(),
        );
    }

    let (url, default_model) = match cfg.provider.as_str() {
        "openai" => (
            "https://api.openai.com/v1/audio/transcriptions".to_string(),
            "whisper-1",
        ),
        "custom" => (cfg.custom_url.trim().to_string(), "whisper-1"),
        _ => (
            "https://api.groq.com/openai/v1/audio/transcriptions".to_string(),
            "whisper-large-v3-turbo",
        ),
    };

    if url.is_empty() {
        return Err("Özel dikte sunucu adresi boş.".to_string());
    }

    let model = if cfg.model.trim().is_empty() {
        default_model.to_string()
    } else {
        cfg.model.trim().to_string()
    };
    let lang = if cfg.language.trim().is_empty() {
        "tr".to_string()
    } else {
        cfg.language.trim().to_string()
    };

    let part = reqwest::multipart::Part::bytes(wav_bytes)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|e| e.to_string())?;

    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model", model)
        .text("language", lang)
        .text("response_format", "json");

    let client = DICTATION_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .pool_max_idle_per_host(1)
            .build()
            .expect("HTTP istemcisi oluşturulamadı")
    });

    let resp = client
        .post(&url)
        .bearer_auth(cfg.api_key.trim())
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Dikte isteği başarısız: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Dikte API hatası ({status}): {}", &body[..body.len().min(200)]));
    }

    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let text = json
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    Ok(text)
}
