pub trait Effect: Send {
    fn process(&mut self, sample: f32) -> f32;

    fn prepare(&mut self, _sample_rate: u32) {}

    fn process_block(&mut self, data: &mut [f32]) {
        for s in data.iter_mut() {
            *s = self.process(*s)
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
            tmp_l: Vec::new(),
            tmp_r: Vec::new(),
        }
    }

    pub fn prepare(&mut self, sample_rate: u32) {
        self.left.prepare(sample_rate);
        self.right.prepare(sample_rate);
    }

    pub fn is_empty(&self) -> bool {
        self.left.is_empty() && self.right.is_empty()
    }

    pub fn process_block_interleaved(&mut self, data: &mut [f32]) {
        self.tmp_l.clear();
        self.tmp_r.clear();
        for pair in data.chunks_exact(2) {
            self.tmp_l.push(pair[0]);
            self.tmp_r.push(pair[1]);
        }

        self.left.process_block(&mut self.tmp_l);
        self.right.process_block(&mut self.tmp_r);

        for (i, pair) in data.chunks_exact_mut(2).enumerate() {
            pair[0] = self.tmp_l[i];
            pair[1] = self.tmp_r[i];
        }
    }
}
