use lire_audio::fx::Effect;
use lire_audio::fx::vinyl::{VinylEffect, VinylParams};

#[test]
fn test_bypass_transparency() {
    let params = VinylParams::default();
    params.set_enabled(false);

    let mut fx = VinylEffect::new(48000, params, 0x12345678);
    fx.prepare(48000);

    // Warm up bypass smoother
    let mut warmup = vec![0.0f32; 4800];
    fx.process_block(&mut warmup);

    // Test with audio input
    let mut signal: Vec<f32> = (0..1024).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
    let original = signal.clone();

    fx.process_block(&mut signal);

    // When bypassed, delay-aligned dry signal matches original amplitude
    for (orig, processed) in original.iter().zip(signal.iter()) {
        assert!(processed.is_finite(), "Output must be finite when bypassed");
        assert!(
            (orig - processed).abs() < 1e-4,
            "Bypass must pass audio transparently: orig={}, proc={}",
            orig,
            processed
        );
    }
}

#[test]
fn test_nan_and_infinity_resilience() {
    let params = VinylParams::default();
    let mut fx = VinylEffect::new(48000, params, 0xABCDEF01);
    fx.prepare(48000);

    let mut poisoned = vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, 1.0];
    fx.process_block(&mut poisoned);

    for (i, &s) in poisoned.iter().enumerate() {
        assert!(
            s.is_finite(),
            "Sample {} must be sanitized and finite, got {}",
            i,
            s
        );
    }

    // Normal audio following poison must process cleanly
    let mut normal = vec![0.25f32; 128];
    fx.process_block(&mut normal);
    for (i, &s) in normal.iter().enumerate() {
        assert!(
            s.is_finite(),
            "Post-poison sample {} must remain finite, got {}",
            i,
            s
        );
    }
}

#[test]
fn test_output_bounded_by_soft_clip() {
    let params = VinylParams::default();
    let mut fx = VinylEffect::new(48000, params, 0xCAFEBABE);
    fx.prepare(48000);

    // Loud input (over 0 dBFS)
    let mut loud: Vec<f32> = (0..2048).map(|i| (i as f32 * 0.05).sin() * 4.0).collect();

    fx.process_block(&mut loud);

    for (i, &s) in loud.iter().enumerate() {
        assert!(
            s.abs() <= 1.15,
            "Soft clipper must keep peaks bounded, got {} at sample {}",
            s,
            i
        );
    }
}

#[test]
fn test_silence_produces_vinyl_noise() {
    let params = VinylParams::default();
    let mut fx = VinylEffect::new(48000, params, 0x12344321);
    fx.prepare(48000);

    // 1 second of silence
    let mut buffer = vec![0.0f32; 48000];
    fx.process_block(&mut buffer);

    let rms = (buffer.iter().map(|&x| x * x).sum::<f32>() / buffer.len() as f32).sqrt();
    let dbfs = 20.0 * rms.log10();

    assert!(
        (-60.0..=-30.0).contains(&dbfs),
        "Vinyl noise on silence should be in analog vinyl range (-60 to -30 dBFS), got {:.2} dBFS",
        dbfs
    );
}

#[test]
fn test_seed_determinism_and_lr_decorrelation() {
    let params = VinylParams::default();

    let mut fx_a = VinylEffect::new(48000, params.clone(), 0x1111);
    let mut fx_b = VinylEffect::new(48000, params.clone(), 0x1111);
    let mut fx_c = VinylEffect::new(48000, params, 0x2222);

    fx_a.prepare(48000);
    fx_b.prepare(48000);
    fx_c.prepare(48000);

    let mut buf_a = vec![0.0f32; 1024];
    let mut buf_b = vec![0.0f32; 1024];
    let mut buf_c = vec![0.0f32; 1024];

    fx_a.process_block(&mut buf_a);
    fx_b.process_block(&mut buf_b);
    fx_c.process_block(&mut buf_c);

    // Identical seeds must match
    for (i, (&a, &b)) in buf_a.iter().zip(buf_b.iter()).enumerate() {
        assert_eq!(a, b, "Identical seed outputs diverged at index {}", i);
    }

    // Different seeds must be decorrelated
    let dot: f32 = buf_a.iter().zip(buf_c.iter()).map(|(a, c)| a * c).sum();
    let norm_a: f32 = buf_a.iter().map(|a| a * a).sum::<f32>().sqrt();
    let norm_c: f32 = buf_c.iter().map(|c| c * c).sum::<f32>().sqrt();
    let corr = dot / (norm_a * norm_c);

    assert!(
        corr.abs() < 0.35,
        "L/R channels with different seeds should be decorrelated, got r = {:.3}",
        corr
    );
}
