use luna_analyzer::dsp::SpectrumAnalyzer;
use std::f32::consts::PI;

const FFT_SIZE: usize = 2048;
const NUM_BANDS: usize = 16;
const SAMPLE_RATE: u32 = 44100;

fn sine_wave(freq: f32, sample_rate: u32, size: usize) -> Vec<f32> {
    (0..size)
        .map(|i| (2.0 * PI * freq * (i as f32) / (sample_rate as f32)).sin())
        .collect()
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

    let quiet = sine_wave(440.0, SAMPLE_RATE, FFT_SIZE)
        .into_iter()
        .map(|s| s * 0.05)
        .collect::<Vec<_>>();

    let loud = sine_wave(440.0, SAMPLE_RATE, FFT_SIZE);

    let quiet_bands = analyzer.analyze(&quiet);
    let loud_bands = analyzer_two.analyze(&loud);

    let quiet_peak = quiet_bands.iter().cloned().fold(0.0f32, f32::max);
    let loud_peak = loud_bands.iter().cloned().fold(0.0f32, f32::max);

    assert!(loud_peak > quiet_peak);
}

#[test]
fn test_bass_vs_treble_isolation() {
    let mut analyzer = SpectrumAnalyzer::new(FFT_SIZE, NUM_BANDS, SAMPLE_RATE);
    let bass = sine_wave(80.0, 44100, 2048);

    for _ in 0..10 {
        analyzer.analyze(&bass);
    }

    let bands = analyzer.analyze(&bass);
    let bass_energy = bands[0..3].iter().copied().fold(0.0f32, f32::max);
    let treble_energy = bands[12..16].iter().copied().fold(0.0f32, f32::max);

    assert!(
        bass_energy > 0.4,
        "Grave de 80Hz não ativou as barras graves!"
    );
    assert!(
        treble_energy < 0.15,
        "Grave vazou indevidamente para agudos!"
    );
}
