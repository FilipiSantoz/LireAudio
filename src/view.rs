pub fn mirrored_bands(bands: &[f32]) -> Vec<f32> {
    if bands.is_empty() {
        return Vec::new();
    }

    let mut mirrored = Vec::with_capacity(bands.len() * 2 - 1);

    mirrored.extend_from_slice(bands);
    mirrored.extend(bands.iter().rev().skip(1));

    mirrored
}
