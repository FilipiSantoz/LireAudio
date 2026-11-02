use lire_audio::dsp::SpectrumAnalyzer;
use std::f32::consts::PI;

const FFT_SIZE: usize = 2048;
const NUM_BANDS: usize = 16;
const SAMPLE_RATE: u32 = 44100;
const AMPL: f32 = 0.20;

fn sine_wave(freq: f32, sample_rate: u32, len: usize, amplitude: f32) -> Vec<f32> {
    (0..len)
        .map(|n| amplitude * (2.0 * PI * freq * n as f32 / sample_rate as f32).sin())
        .collect()
}

fn settle(analyzer: &mut SpectrumAnalyzer, signal: &[f32], hops: usize) -> Vec<f32> {
    let mut last = Vec::new();
    for _ in 0..hops {
        last = analyzer.analyze(signal).to_vec();
    }
    last
}

fn peak_band_index(bands: &[f32]) -> usize {
    bands
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

#[test]
fn test_silence_produces_no_energy() {
    let mut analyzer = SpectrumAnalyzer::new(FFT_SIZE, NUM_BANDS, SAMPLE_RATE);
    let silence = vec![0.0f32; FFT_SIZE];

    let bands = settle(&mut analyzer, &silence, 10);
    for &val in &bands {
        assert!(val < 0.01, "Silêncio acusou energia: {val}");
    }
}

#[test]
fn louder_signal_should_produce_higher_bands() {
    let mut quiet_an = SpectrumAnalyzer::new(FFT_SIZE, NUM_BANDS, SAMPLE_RATE);
    let mut loud_an = SpectrumAnalyzer::new(FFT_SIZE, NUM_BANDS, SAMPLE_RATE);

    let quiet = sine_wave(440.0, SAMPLE_RATE, FFT_SIZE, AMPL * 0.15); // ~ -30 dBFS
    let loud = sine_wave(440.0, SAMPLE_RATE, FFT_SIZE, AMPL); // ~ -14 dBFS

    let quiet_bands = settle(&mut quiet_an, &quiet, 30);
    let loud_bands = settle(&mut loud_an, &loud, 30);

    let quiet_peak = quiet_bands.iter().copied().fold(0.0f32, f32::max);
    let loud_peak = loud_bands.iter().copied().fold(0.0f32, f32::max);

    assert!(
        quiet_peak > 0.0 && loud_peak > quiet_peak,
        "Sinal mais alto deveria produzir pico maior: quiet={quiet_peak}, loud={loud_peak}"
    );
}

#[test]
fn test_bass_vs_treble_isolation() {
    let mut analyzer = SpectrumAnalyzer::new(FFT_SIZE, NUM_BANDS, SAMPLE_RATE);
    let bass = sine_wave(80.0, SAMPLE_RATE, FFT_SIZE, AMPL);

    let bands = settle(&mut analyzer, &bass, 20);
    let bass_energy = bands[0..4].iter().copied().fold(0.0f32, f32::max);
    let treble_energy = bands[12..16].iter().copied().fold(0.0f32, f32::max);

    assert!(
        bass_energy > 0.2 && bass_energy > treble_energy * 3.0,
        "Grave de 80Hz não se destacou dos agudos: bass={bass_energy}, treble={treble_energy}"
    );
}

#[test]
fn frequency_ordering_across_spectrum() {
    let freqs = [80.0, 1000.0, 8000.0];
    let mut peak_indices = Vec::new();

    for &freq in &freqs {
        let mut analyzer = SpectrumAnalyzer::new(FFT_SIZE, NUM_BANDS, SAMPLE_RATE);
        let signal = sine_wave(freq, SAMPLE_RATE, FFT_SIZE, AMPL);
        let bands = settle(&mut analyzer, &signal, 20);
        let peak_val = bands.iter().copied().fold(0.0f32, f32::max);
        assert!(peak_val > 0.05, "Frequência {freq}Hz não gerou energia");
        peak_indices.push(peak_band_index(&bands));
    }

    assert!(
        peak_indices[0] < peak_indices[1] && peak_indices[1] < peak_indices[2],
        "As bandas de pico devem crescer com a frequência (80Hz < 1kHz < 8kHz): {peak_indices:?}"
    );
}

#[test]
fn envelope_releases_smoothly_to_zero_after_signal_stops() {
    let mut analyzer = SpectrumAnalyzer::new(FFT_SIZE, NUM_BANDS, SAMPLE_RATE);
    let tone = sine_wave(200.0, SAMPLE_RATE, FFT_SIZE, AMPL);
    let silence = vec![0.0f32; FFT_SIZE];

    let active_bands = settle(&mut analyzer, &tone, 30);
    let active_peak = active_bands.iter().copied().fold(0.0f32, f32::max);

    let decaying_bands = settle(&mut analyzer, &silence, 5);
    let decaying_peak = decaying_bands.iter().copied().fold(0.0f32, f32::max);

    let settled_silence = settle(&mut analyzer, &silence, 120);
    let final_peak = settled_silence.iter().copied().fold(0.0f32, f32::max);

    assert!(
        active_peak > decaying_peak && decaying_peak > final_peak && final_peak < 0.02,
        "Envelope de release inválido: active={active_peak:.3}, decaying={decaying_peak:.3}, final={final_peak:.3}"
    );
}
