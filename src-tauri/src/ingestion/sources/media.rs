//! 音视频文件 → symphonia 解码为 mono PCM f32 → STT → 文本。

use crate::ingestion::job::IngestError;
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// 解码任意支持的容器/编码为 (mono f32 samples, sample_rate)。
pub fn decode_to_pcm(path: &str) -> Result<(Vec<f32>, u32), IngestError> {
    let file = std::fs::File::open(path).map_err(|e| IngestError::Io(e.to_string()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.rsplit('.').next() {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| IngestError::Decode(format!("probe: {e}")))?;
    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .ok_or_else(|| IngestError::Decode("no audio track".into()))?;
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(16_000);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| IngestError::Decode(format!("make decoder: {e}")))?;

    let mut samples: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(_) => break,
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => append_mono_f32(&decoded, &mut samples),
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(IngestError::Decode(e.to_string())),
        }
    }
    if samples.is_empty() {
        return Err(IngestError::Decode("decoded 0 samples".into()));
    }
    Ok((samples, sample_rate))
}

/// 把一个解码缓冲(可能多声道)平均成 mono f32 追加进 out。
fn append_mono_f32(decoded: &AudioBufferRef, out: &mut Vec<f32>) {
    match decoded {
        AudioBufferRef::F32(buf) => mix_planar(buf.spec().channels.count(), buf.frames(), out, |ch, fr| buf.chan(ch)[fr]),
        AudioBufferRef::S16(buf) => mix_planar(buf.spec().channels.count(), buf.frames(), out, |ch, fr| buf.chan(ch)[fr] as f32 / 32768.0),
        AudioBufferRef::S32(buf) => mix_planar(buf.spec().channels.count(), buf.frames(), out, |ch, fr| buf.chan(ch)[fr] as f32 / 2147483648.0),
        AudioBufferRef::U8(buf) => mix_planar(buf.spec().channels.count(), buf.frames(), out, |ch, fr| (buf.chan(ch)[fr] as f32 - 128.0) / 128.0),
        _ => {}
    }
}

fn mix_planar<F: Fn(usize, usize) -> f32>(channels: usize, frames: usize, out: &mut Vec<f32>, get: F) {
    if channels == 0 { return; }
    for fr in 0..frames {
        let mut acc = 0.0f32;
        for ch in 0..channels {
            acc += get(ch, fr);
        }
        out.push(acc / channels as f32);
    }
}

/// 文件 → 文本(解码 + STT)。
pub async fn transcribe_media(path: &str) -> Result<String, IngestError> {
    let (pcm, sample_rate) = decode_to_pcm(path)?;
    let res = crate::stt::transcribe_samples(pcm, sample_rate, None)
        .await
        .map_err(IngestError::Stt)?;
    Ok(res.text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// 生成一个最小 16-bit PCM mono WAV(8000Hz, 4 帧)写到临时文件,返回句柄。
    fn write_tiny_wav() -> tempfile::NamedTempFile {
        let sample_rate: u32 = 8000;
        let samples: [i16; 4] = [0, 16000, -16000, 8000];
        let data_len = (samples.len() * 2) as u32;
        let mut f = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        let w = f.as_file_mut();
        w.write_all(b"RIFF").unwrap();
        w.write_all(&(36 + data_len).to_le_bytes()).unwrap();
        w.write_all(b"WAVE").unwrap();
        w.write_all(b"fmt ").unwrap();
        w.write_all(&16u32.to_le_bytes()).unwrap();
        w.write_all(&1u16.to_le_bytes()).unwrap();
        w.write_all(&1u16.to_le_bytes()).unwrap();
        w.write_all(&sample_rate.to_le_bytes()).unwrap();
        w.write_all(&(sample_rate * 2).to_le_bytes()).unwrap();
        w.write_all(&2u16.to_le_bytes()).unwrap();
        w.write_all(&16u16.to_le_bytes()).unwrap();
        w.write_all(b"data").unwrap();
        w.write_all(&data_len.to_le_bytes()).unwrap();
        for s in samples { w.write_all(&s.to_le_bytes()).unwrap(); }
        w.flush().unwrap();
        f
    }

    #[test]
    fn decodes_tiny_wav_to_pcm() {
        let f = write_tiny_wav();
        let (pcm, sr) = decode_to_pcm(f.path().to_str().unwrap()).unwrap();
        assert_eq!(sr, 8000);
        assert_eq!(pcm.len(), 4);
        assert!(pcm.iter().any(|&x| x != 0.0));
    }
}
