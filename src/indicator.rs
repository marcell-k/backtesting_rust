pub struct Indicator {
    pub name: String,
    pub values: Vec<f64>,
    pub plot: bool,
    pub overlay: bool,
    pub scatter: bool,
    warmup_bars: usize,
}

impl Indicator {
    pub fn new(name: impl Into<String>, values: Vec<f64>) -> Self {
        let warmup_bars = values.iter().take_while(|x| x.is_nan()).count();
        Self {
            name: name.into(),
            values,
            plot: true,
            overlay: false,
            scatter: false,
            warmup_bars,
        }
    }

    pub fn plot(mut self, plot: bool) -> Self {
        self.plot = plot;
        self
    }

    pub fn overlay(mut self, overlay: bool) -> Self {
        self.overlay = overlay;
        self
    }

    pub fn scatter(mut self, scatter: bool) -> Self {
        self.scatter = scatter;
        self
    }

    pub fn warmup_bars(&self) -> usize {
        self.warmup_bars
    }

    pub fn as_of(&self, index: usize) -> &[f64] {
        &self.values[..=index.min(self.values.len().saturating_sub(1))]
    }

    pub fn value_at(&self, index: usize) -> f64 {
        self.values[index]
    }
}
pub fn warmup_start_bar(indicators: &[Indicator]) -> usize {
    1 + indicators
        .iter()
        .map(|i| i.warmup_bars())
        .max()
        .unwrap_or(0)
}
