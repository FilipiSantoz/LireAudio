use lire_audio::dsp::SpectrumAnalyzer;
use std::f32::consts::PI;

const FFT_SIZE: usize = 2048;
const NUM_BANDS: usize = 16;
const SAMPLE_RATE: u32 = 44100;
const AMPL: f32 = 0.0005;

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

#[test]
fn test_silence_produces_no_energy() {
    let mut analyzer = SpectrumAnalyzer::new(FFT_SIZE, NUM_BANDS, SAMPLE_RATE);
    let silence = vec![0.0f32; 2048];

    for _ in 0..5 {
        analyzer.analyze(&silence);
    }

    let bands = analyzer.analyze(&silence);
    for &val in bands {
        assert!(val < 0.05, "Silêncio acusou energia: {val}");
    }
}

#[test]
fn louder_signal_should_produce_higher_bands() {
    let mut analyzer = SpectrumAnalyzer::new(FFT_SIZE, NUM_BANDS, SAMPLE_RATE);
    let mut analyzer_two = SpectrumAnalyzer::new(FFT_SIZE, NUM_BANDS, SAMPLE_RATE);

    let quiet = sine_wave(440.0, SAMPLE_RATE, FFT_SIZE, AMPL)
        .into_iter()
        .map(|s| s * 0.05)
        .collect::<Vec<_>>();

    let loud = sine_wave(440.0, SAMPLE_RATE, FFT_SIZE, AMPL);

    let quiet_bands = analyzer.analyze(&quiet);
    let loud_bands = analyzer_two.analyze(&loud);

    let quiet_peak = quiet_bands.iter().cloned().fold(0.0f32, f32::max);
    let loud_peak = loud_bands.iter().cloned().fold(0.0f32, f32::max);

    assert!(loud_peak > quiet_peak);
}

#[test]
fn test_bass_vs_treble_isolation() {
    let mut analyzer = SpectrumAnalyzer::new(FFT_SIZE, NUM_BANDS, SAMPLE_RATE);
    let bass = sine_wave(80.0, SAMPLE_RATE, FFT_SIZE, AMPL);

    for _ in 0..10 {
        analyzer.analyze(&bass);
    }

    let bands = analyzer.analyze(&bass);
    let bass_energy = bands[0..3].iter().copied().fold(0.0f32, f32::max);
    let treble_energy = bands[12..16].iter().copied().fold(0.0f32, f32::max);

    assert!(
        bass_energy > treble_energy * 3.0,
        "grave de 80Hz não se destacou o suficiente dos agudos: bass={bass_energy}, treble={treble_energy}"
    );
}

#[test]
fn level_difference_maps_proportionally_in_db() {
    let mut loud_an =
        SpectrumAnalyzer::new(FFT_SIZE, NUM_BANDS, SAMPLE_RATE).fixed_range(-80.0, 0.0);
    let mut quiet_an =
        SpectrumAnalyzer::new(FFT_SIZE, NUM_BANDS, SAMPLE_RATE).fixed_range(-80.0, 0.0);

    let loud = sine_wave(440.0, SAMPLE_RATE, FFT_SIZE, 0.1); // -20 dBFS
    let quiet = sine_wave(440.0, SAMPLE_RATE, FFT_SIZE, 0.01); // -40 dBFS

    let loud_bands = settle(&mut loud_an, &loud, 60);
    let quiet_bands = settle(&mut quiet_an, &quiet, 60);

    let loud_peak = loud_bands.iter().cloned().fold(0.0f32, f32::max);
    let quiet_peak = quiet_bands.iter().cloned().fold(0.0f32, f32::max);

    let diff = loud_peak - quiet_peak;
    assert!(
        (diff - 0.25).abs() < 0.02,
        "diferença de {diff:.3}, esperado ~0.25 (20 dB em escala de 80 dB)"
    );
}

#[test]
fn dominant_frequency_lands_in_expected_band() {
    let mut analyzer = SpectrumAnalyzer::new(FFT_SIZE, NUM_BANDS, SAMPLE_RATE);
    let signal = sine_wave(1000.0, SAMPLE_RATE, FFT_SIZE, AMPL);

    let bands = analyzer.analyze(&signal);
    let (loudest, _) = bands
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .unwrap();

    assert!(
        (8..=12).contains(&loudest),
        "esperava o pico perto do meio, veio na banda {loudest}"
    );
}
