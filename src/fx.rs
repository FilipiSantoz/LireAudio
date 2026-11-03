pub mod vinyl;

pub use vinyl::VinylEffect;
pub trait Effect: Send {
    fn process(&mut self, sample: f32) -> f32;

    fn prepare(&mut self, _sample_rate: u32) {}

    fn process_block(&mut self, data: &mut [f32]) {
        for s in data.iter_mut() {
            *s = self.process(*s);
        }
    }
}

#[derive(Default)]
pub struct FxChain {
    effects: Vec<Box<dyn Effect>>,
}

impl FxChain {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add<E: Effect + 'static>(&mut self, effect: E) {
        self.effects.push(Box::new(effect));
    }

    pub fn process(&mut self, sample: f32) -> f32 {
        self.effects.iter_mut().fold(sample, |s, fx| fx.process(s))
    }

    pub fn prepare(&mut self, sample_rate: u32) {
        for fx in &mut self.effects {
            fx.prepare(sample_rate);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    pub fn process_block(&mut self, data: &mut [f32]) {
        for fx in &mut self.effects {
            fx.process_block(data);
        }
    }
}

pub const MAX_BLOCK_FRAMES: usize = 8192;

pub struct StereoFxChain {
    left: FxChain,
    right: FxChain,
    tmp_l: Vec<f32>,
    tmp_r: Vec<f32>,
}

impl StereoFxChain {
    pub fn new(left: FxChain, right: FxChain) -> Self {
        Self {
            left,
            right,
            tmp_l: vec![0.0; MAX_BLOCK_FRAMES],
            tmp_r: vec![0.0; MAX_BLOCK_FRAMES],
        }
    }

    pub fn prepare(&mut self, sample_rate: u32) {
        self.left.prepare(sample_rate);
        self.right.prepare(sample_rate);
        if self.tmp_l.len() < MAX_BLOCK_FRAMES {
            self.tmp_l.resize(MAX_BLOCK_FRAMES, 0.0);
        }
        if self.tmp_r.len() < MAX_BLOCK_FRAMES {
            self.tmp_r.resize(MAX_BLOCK_FRAMES, 0.0);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.left.is_empty() && self.right.is_empty()
    }

    pub fn process_block_interleaved(&mut self, data: &mut [f32]) {
        let max_chunk = self.tmp_l.len().min(self.tmp_r.len()).max(1);
        for block in data.as_chunks_mut::<2>().0.chunks_mut(max_chunk) {
            let n = block.len();
            let l_buf = &mut self.tmp_l[..n];
            let r_buf = &mut self.tmp_r[..n];

            for (i, &[l, r]) in block.iter().enumerate() {
                l_buf[i] = l;
                r_buf[i] = r;
            }

            self.left.process_block(l_buf);
            self.right.process_block(r_buf);

            for (i, [l, r]) in block.iter_mut().enumerate() {
                *l = l_buf[i];
                *r = r_buf[i];
            }
        }
    }
}
