//! sherpa-onnx streaming Zipformer recognition engine.
//!
//! This module is compiled when the `sherpa-engine` Cargo feature is enabled:
//!
//!   cargo build --features sherpa-engine
//!
//! It uses `sherpa_rs_sys` directly for the online (streaming) recognizer API,
//! which supports true streaming Zipformer transducer models.
//!
//! # Required model files
//! Download a streaming Zipformer transducer model from:
//!   https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models
//!
//! A model archive contains versioned filenames.  Point each config field at
//! the actual file inside the extracted directory, for example:
//!
//! ```toml
//! realtime_engine = "sherpa-onnx"
//! sherpa_encoder  = "./models/sherpa-onnx-streaming-zipformer-en-2023-06-26/encoder-epoch-99-avg-1-chunk-16-left-128.int8.onnx"
//! sherpa_decoder  = "./models/sherpa-onnx-streaming-zipformer-en-2023-06-26/decoder-epoch-99-avg-1-chunk-16-left-128.onnx"
//! sherpa_joiner   = "./models/sherpa-onnx-streaming-zipformer-en-2023-06-26/joiner-epoch-99-avg-1-chunk-16-left-128.int8.onnx"
//! sherpa_tokens   = "./models/sherpa-onnx-streaming-zipformer-en-2023-06-26/tokens.txt"
//! ```

use anyhow::{Context, Result};
use chrono::Local;
use std::ffi::{CStr, CString};
use std::mem;
use std::sync::mpsc;

use sherpa_rs::sherpa_rs_sys as sys;

use crate::recognition::{RealtimeRecognizer, RecognizedText};

// ── SherpaOnnxRecognizer ──────────────────────────────────────────────────────

/// Wraps the sherpa-onnx *online* (streaming) recognizer via raw sys bindings.
pub struct SherpaOnnxRecognizer {
    recognizer: *const sys::SherpaOnnxOnlineRecognizer,
    stream:     *const sys::SherpaOnnxOnlineStream,
    text_sender: mpsc::Sender<RecognizedText>,
    sample_rate: i32,
    last_partial: String,
}

// The raw pointers are not Send by default; we manage them exclusively from
// the recognition thread, so this is safe.
unsafe impl Send for SherpaOnnxRecognizer {}

impl SherpaOnnxRecognizer {
    pub fn new(
        encoder:     &str,
        decoder:     &str,
        joiner:      &str,
        tokens:      &str,
        sample_rate: u32,
        text_sender: mpsc::Sender<RecognizedText>,
    ) -> Result<Self> {
        // CStrings must live until after SherpaOnnxCreateOnlineRecognizer returns
        let c_encoder        = CString::new(encoder).context("encoder path contains nul")?;
        let c_decoder        = CString::new(decoder).context("decoder path contains nul")?;
        let c_joiner         = CString::new(joiner).context("joiner path contains nul")?;
        let c_tokens         = CString::new(tokens).context("tokens path contains nul")?;
        let c_greedy         = CString::new("greedy_search").unwrap();
        let c_cpu            = CString::new("cpu").unwrap();
        let c_empty          = CString::new("").unwrap();

        let recognizer = unsafe {
            // Build the full config with zeroed optional fields
            let mut cfg: sys::SherpaOnnxOnlineRecognizerConfig = mem::zeroed();

            // Feature extraction
            cfg.feat_config.sample_rate = sample_rate as i32;
            cfg.feat_config.feature_dim = 80; // standard mel filterbank for zipformer

            // Transducer model paths
            cfg.model_config.transducer.encoder = c_encoder.as_ptr();
            cfg.model_config.transducer.decoder = c_decoder.as_ptr();
            cfg.model_config.transducer.joiner  = c_joiner.as_ptr();

            // Shared model settings
            cfg.model_config.tokens       = c_tokens.as_ptr();
            cfg.model_config.num_threads  = 2;
            cfg.model_config.provider     = c_cpu.as_ptr();
            cfg.model_config.debug        = 0;
            cfg.model_config.model_type   = c_empty.as_ptr();
            cfg.model_config.modeling_unit = c_empty.as_ptr();
            cfg.model_config.bpe_vocab    = c_empty.as_ptr();

            // Decoding
            cfg.decoding_method    = c_greedy.as_ptr();
            cfg.max_active_paths   = 4;

            // Endpoint detection
            // rule1: emit after this many seconds of trailing silence (any utterance)
            // rule2: emit after this many seconds of trailing silence (utterance already has words)
            // rule3: force-emit after this many seconds regardless
            cfg.enable_endpoint              = 1;
            cfg.rule1_min_trailing_silence   = 1.2;
            cfg.rule2_min_trailing_silence   = 0.6;
            cfg.rule3_min_utterance_length   = 10.0;

            // Hotwords disabled
            cfg.hotwords_file  = c_empty.as_ptr();
            cfg.hotwords_score = 1.5;
            cfg.hotwords_buf   = c_empty.as_ptr();
            cfg.hotwords_buf_size = 0;

            // blank_penalty, rule_fsts, rule_fars, ctc_fst_decoder_config,
            // hr (homophone replacer) — all stay zeroed / null

            let r = sys::SherpaOnnxCreateOnlineRecognizer(&cfg);
            // cfg (and all CStrings) are still alive here — we're still in the unsafe block
            r
        };

        if recognizer.is_null() {
            anyhow::bail!(
                "Failed to create sherpa-onnx online recognizer.\n\
                 Check that the model files exist and are valid online transducer models."
            );
        }

        let stream = unsafe { sys::SherpaOnnxCreateOnlineStream(recognizer) };
        if stream.is_null() {
            unsafe { sys::SherpaOnnxDestroyOnlineRecognizer(recognizer); }
            anyhow::bail!("Failed to create sherpa-onnx online stream");
        }

        log::info!(
            "sherpa-onnx online recognizer ready (sample_rate: {} Hz)",
            sample_rate
        );

        Ok(Self {
            recognizer,
            stream,
            text_sender,
            sample_rate: sample_rate as i32,
            last_partial: String::new(),
        })
    }

    /// Drain decoder until no more frames are available.
    unsafe fn decode_ready_frames(&self) {
        while sys::SherpaOnnxIsOnlineStreamReady(self.recognizer, self.stream) != 0 {
            sys::SherpaOnnxDecodeOnlineStream(self.recognizer, self.stream);
        }
    }

    /// Get the current partial result text (caller must free via sys).
    unsafe fn get_text(&self) -> String {
        let result_ptr = sys::SherpaOnnxGetOnlineStreamResult(self.recognizer, self.stream);
        if result_ptr.is_null() {
            return String::new();
        }
        let text = if (*result_ptr).text.is_null() {
            String::new()
        } else {
            CStr::from_ptr((*result_ptr).text)
                .to_string_lossy()
                .trim()
                .to_string()
        };
        sys::SherpaOnnxDestroyOnlineRecognizerResult(result_ptr);
        text
    }

    fn emit_and_reset(&mut self, is_final: bool) {
        let text = unsafe { self.get_text() };
        if !text.is_empty() {
            // \r\x1b[K clears the partial line; \r\n is the correct newline in raw mode
            if is_final {
                print!("\r\x1b[K\u{1f3a4} Final: {}\r\n", text);
            } else {
                print!("\r\x1b[K\u{1f3a4} Recognized: {}\r\n", text);
            }
            let _ = std::io::Write::flush(&mut std::io::stdout());
            let _ = self.text_sender.send(RecognizedText {
                text,
                timestamp: Local::now(),
                is_final,
            });
        }
        unsafe { sys::SherpaOnnxOnlineStreamReset(self.recognizer, self.stream); }
        self.last_partial.clear();
    }
}

impl RealtimeRecognizer for SherpaOnnxRecognizer {
    fn process_audio(&mut self, samples: &[f32]) -> Result<()> {
        if samples.is_empty() {
            return Ok(());
        }
        unsafe {
            sys::SherpaOnnxOnlineStreamAcceptWaveform(
                self.stream,
                self.sample_rate,
                samples.as_ptr(),
                samples.len() as i32,
            );
            self.decode_ready_frames();
            if sys::SherpaOnnxOnlineStreamIsEndpoint(self.recognizer, self.stream) != 0 {
                self.emit_and_reset(false);
            } else {
                let partial = self.get_text();
                if !partial.is_empty() && partial != self.last_partial {
                    // Truncate to ~100 chars so the partial never wraps to a second line
                    // (\r\x1b[K only clears the current row, so multi-row wraps leave residue)
                    let end = partial.char_indices().nth(100).map(|(i, _)| i).unwrap_or(partial.len());
                    print!("\r\x1b[K\u{1f50a} {}", &partial[..end]);
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                    self.last_partial = partial;
                }
            }
        }
        Ok(())
    }

    fn finalize(&mut self) -> Result<()> {
        // Feed 100 ms of silence then signal end-of-input
        let silence = vec![0.0f32; (self.sample_rate / 10) as usize];
        unsafe {
            sys::SherpaOnnxOnlineStreamAcceptWaveform(
                self.stream,
                self.sample_rate,
                silence.as_ptr(),
                silence.len() as i32,
            );
            sys::SherpaOnnxOnlineStreamInputFinished(self.stream);
            self.decode_ready_frames();
        }
        self.emit_and_reset(true);
        Ok(())
    }
}

impl Drop for SherpaOnnxRecognizer {
    fn drop(&mut self) {
        unsafe {
            sys::SherpaOnnxDestroyOnlineStream(self.stream);
            sys::SherpaOnnxDestroyOnlineRecognizer(self.recognizer);
        }
    }
}
