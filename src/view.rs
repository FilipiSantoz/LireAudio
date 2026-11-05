use crate::pipeline::NUM_BANDS;

pub const MIRRORED_BANDS: usize = NUM_BANDS * 2 - 1;

pub fn mirrored_bands(bands: &[f32; NUM_BANDS]) -> [f32; MIRRORED_BANDS] {
    let mut mirrored = [0.0f32; MIRRORED_BANDS];

    mirrored[..NUM_BANDS].copy_from_slice(bands);

    for (dest, &src) in mirrored[NUM_BANDS..]
        .iter_mut()
        .zip(bands[..NUM_BANDS - 1].iter().rev())
    {
        *dest = src;
    }

    mirrored
}
