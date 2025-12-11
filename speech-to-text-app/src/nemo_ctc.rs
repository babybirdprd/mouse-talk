//! NeMo CTC Recognizer for sherpa-onnx offline speech recognition
//! 
//! This module provides support for NeMo CTC-based models like the
//! NVIDIA Parakeet TDT-CTC 110M model that use a single model.onnx file.
//!
//! To add this to sherpa-rs fork (babybirdprd/sherpa-rs):
//! 1. Copy this file to crates/sherpa-rs/src/nemo_ctc.rs
//! 2. Add `pub mod nemo_ctc;` to crates/sherpa-rs/src/lib.rs

use std::ffi::CString;
use std::mem;
use std::ptr::null;

use eyre::{bail, Result};
use sherpa_rs_sys;

fn cstring_from_str(s: &str) -> CString {
    CString::new(s).unwrap_or_default()
}

fn get_default_provider() -> String {
    "cpu".into()
}

#[derive(Debug)]
pub struct NemoCtcRecognizer {
    recognizer: *const sherpa_rs_sys::SherpaOnnxOfflineRecognizer,
}

#[derive(Debug, Clone)]
pub struct NemoCtcConfig {
    /// Path to the ONNX model file (e.g., model.int8.onnx)
    pub model: String,
    /// Path to the tokens file
    pub tokens: String,
    /// Execution provider (e.g., "cpu", "cuda")
    pub provider: Option<String>,
    /// Number of threads
    pub num_threads: Option<i32>,
    /// Enable debug mode
    pub debug: bool,
}

impl Default for NemoCtcConfig {
    fn default() -> Self {
        Self {
            model: String::new(),
            tokens: String::new(),
            debug: false,
            provider: None,
            num_threads: Some(4),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NemoCtcResult {
    pub lang: String,
    pub text: String,
    pub timestamps: Vec<f32>,
    pub tokens: Vec<String>,
}

impl NemoCtcRecognizer {
    pub fn new(config: NemoCtcConfig) -> Result<Self> {
        let debug = if config.debug { 1 } else { 0 };
        let provider = config.provider.unwrap_or_else(get_default_provider);

        // Prepare C strings
        let provider_ptr = cstring_from_str(&provider);
        let model_ptr = cstring_from_str(&config.model);
        let tokens_ptr = cstring_from_str(&config.tokens);
        let decoding_method_ptr = cstring_from_str("greedy_search");

        // NeMo CTC model config - uses single model file
        let nemo_ctc_config = sherpa_rs_sys::SherpaOnnxOfflineNemoEncDecCtcModelConfig {
            model: model_ptr.as_ptr(),
        };

        // Offline model config
        let model_config = unsafe {
            sherpa_rs_sys::SherpaOnnxOfflineModelConfig {
                debug,
                num_threads: config.num_threads.unwrap_or(4),
                provider: provider_ptr.as_ptr(),
                tokens: tokens_ptr.as_ptr(),
                nemo_ctc: nemo_ctc_config,
                
                // Null out other model types
                bpe_vocab: mem::zeroed::<_>(),
                model_type: mem::zeroed::<_>(),
                modeling_unit: mem::zeroed::<_>(),
                paraformer: mem::zeroed::<_>(),
                tdnn: mem::zeroed::<_>(),
                telespeech_ctc: null(),
                fire_red_asr: mem::zeroed::<_>(),
                transducer: mem::zeroed::<_>(),
                whisper: mem::zeroed::<_>(),
                sense_voice: mem::zeroed::<_>(),
                moonshine: mem::zeroed::<_>(),
                dolphin: mem::zeroed::<_>(),
                zipformer_ctc: mem::zeroed::<_>(),
                canary: mem::zeroed::<_>(),
                wenet_ctc: mem::zeroed::<_>(),
            }
        };

        // Recognizer config
        let recognizer_config = unsafe {
            sherpa_rs_sys::SherpaOnnxOfflineRecognizerConfig {
                decoding_method: decoding_method_ptr.as_ptr(),
                feat_config: sherpa_rs_sys::SherpaOnnxFeatureConfig {
                    sample_rate: 16000,
                    feature_dim: 80,
                },
                model_config,
                hotwords_file: null(),
                hotwords_score: 0.0,
                lm_config: mem::zeroed::<_>(),
                max_active_paths: 0,
                rule_fars: null(),
                rule_fsts: null(),
                blank_penalty: 0.0,
                hr: mem::zeroed::<_>(),
            }
        };

        let recognizer =
            unsafe { sherpa_rs_sys::SherpaOnnxCreateOfflineRecognizer(&recognizer_config) };
        
        if recognizer.is_null() {
            bail!("Failed to create NeMo CTC recognizer. Check model path and tokens file.");
        }

        Ok(Self { recognizer })
    }

    pub fn transcribe(&mut self, sample_rate: u32, samples: &[f32]) -> NemoCtcResult {
        unsafe {
            let stream = sherpa_rs_sys::SherpaOnnxCreateOfflineStream(self.recognizer);
            sherpa_rs_sys::SherpaOnnxAcceptWaveformOffline(
                stream,
                sample_rate as i32,
                samples.as_ptr(),
                samples.len() as i32,
            );
            sherpa_rs_sys::SherpaOnnxDecodeOfflineStream(self.recognizer, stream);
            let result_ptr = sherpa_rs_sys::SherpaOnnxGetOfflineStreamResult(stream);
            let raw_result = result_ptr.read();
            
            let lang = cstr_to_string(raw_result.lang);
            let text = cstr_to_string(raw_result.text);
            let count = raw_result.count.try_into().unwrap_or(0);
            
            let timestamps = if raw_result.timestamps.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(raw_result.timestamps, count).to_vec()
            };
            
            let mut tokens = Vec::with_capacity(count);
            if !raw_result.tokens.is_null() {
                let mut next_token = raw_result.tokens;
                for _ in 0..count {
                    let token = std::ffi::CStr::from_ptr(next_token);
                    tokens.push(token.to_string_lossy().into_owned());
                    next_token = next_token
                        .wrapping_byte_offset(token.to_bytes_with_nul().len().try_into().unwrap());
                }
            }
            
            let result = NemoCtcResult {
                lang,
                text,
                timestamps,
                tokens,
            };

            sherpa_rs_sys::SherpaOnnxDestroyOfflineRecognizerResult(result_ptr);
            sherpa_rs_sys::SherpaOnnxDestroyOfflineStream(stream);

            result
        }
    }
}

unsafe fn cstr_to_string(ptr: *const std::os::raw::c_char) -> String {
    if ptr.is_null() {
        String::new()
    } else {
        std::ffi::CStr::from_ptr(ptr)
            .to_string_lossy()
            .into_owned()
    }
}

unsafe impl Send for NemoCtcRecognizer {}
unsafe impl Sync for NemoCtcRecognizer {}

impl Drop for NemoCtcRecognizer {
    fn drop(&mut self) {
        unsafe {
            sherpa_rs_sys::SherpaOnnxDestroyOfflineRecognizer(self.recognizer);
        }
    }
}
