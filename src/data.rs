use chrono::NaiveDateTime;

#[derive(Debug, Copy, PartialEq, Eq, Clone)]
pub enum Field {
    Open,
    High,
    Low,
    Close,
    Volume,
}
type Array = Vec<f64>;

#[derive(Debug, Clone)]
pub struct Data {
    pub index: Vec<NaiveDateTime>,
    open: Array,
    high: Array,
    low: Array,
    close: Array,
    volume: Array,
    curr: usize,
}
impl Data {
    pub fn new(
        index: Vec<NaiveDateTime>,
        open: Array,
        high: Array,
        low: Array,
        close: Array,
        volume: Array,
    ) -> Self {
        let n = index.len();
        assert!(n > 0, "data is empty");
        assert!(
            [open.len(), high.len(), low.len(), close.len(), volume.len()]
                .iter()
                .all(|a| *a == n),
            "All OHLCV must have the same length"
        );

        Self {
            index,
            open,
            high,
            low,
            close,
            volume,
            curr: n,
        }
    }

    pub fn full_len(&self) -> usize {
        self.index.len()
    }
    pub fn is_empty(&self) -> bool {
        self.curr == 0
    }
    pub fn len(&self) -> usize {
        self.curr
    }

    pub fn set_length(&mut self, n: usize) {
        assert!(n <= self.full_len());
        self.curr = n
    }

    pub fn open(&self) -> &[f64] {
        &self.open[..self.curr]
    }
    pub fn high(&self) -> &[f64] {
        &self.high[..self.curr]
    }
    pub fn low(&self) -> &[f64] {
        &self.low[..self.curr]
    }
    pub fn close(&self) -> &[f64] {
        &self.close[..self.curr]
    }
    pub fn volume(&self) -> &[f64] {
        &self.volume[..self.curr]
    }
    pub fn index(&self) -> &[NaiveDateTime] {
        &self.index[..self.curr]
    }

    /// Only used for `stats` and `plotting`
    pub fn full_close(&self) -> &[f64] {
        &self.close
    }

    pub fn at(&self, field: Field, offset: isize) -> f64 {
        let index = match usize::try_from(self.curr as isize + offset) {
            Ok(idx) if idx < self.curr => idx,
            _ => return f64::NAN,
        };
        match field {
            Field::Open => self.open[index],
            Field::High => self.high[index],
            Field::Low => self.low[index],
            Field::Close => self.close[index],
            Field::Volume => self.volume[index],
        }
    }
}
