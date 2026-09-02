#[derive(Clone)]
pub enum Commission {
    /// `fixed` dollar amount + relative `fraction` of order notional
    FixedRelative { fixed: f64, relative: f64 },
    /// arbitary `fn(order_size, price) -> commission in cash`
    Custom(std::sync::Arc<dyn Fn(f64, f64) -> f64 + Send + Sync>),
}

impl Default for Commission {
    fn default() -> Self {
        Commission::relative(0.0)
    }
}
impl Commission {
    pub fn relative(rate: f64) -> Self {
        Commission::FixedRelative {
            fixed: 0.0,
            relative: rate,
        }
    }
    pub fn fixed_and_relative(fixed: f64, rate: f64) -> Self {
        Commission::FixedRelative {
            fixed,
            relative: rate,
        }
    }

    pub fn custom(f: impl Fn(f64, f64) -> f64 + Send + Sync + 'static) -> Self {
        Commission::Custom(std::sync::Arc::new(f))
    }

    pub fn compute(&self, order_size: f64, price: f64) -> f64 {
        match self {
            Commission::FixedRelative { fixed, relative } => {
                fixed + order_size.abs() * price * relative
            }
            Commission::Custom(f) => f(order_size, price),
        }
    }
}

impl std::fmt::Debug for Commission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Commission::FixedRelative { fixed, relative } => f
                .debug_struct("Commission::FixedRelative")
                .field("fixed", fixed)
                .field("relative", relative)
                .finish(),

            Commission::Custom(_) => write!(f, "Commission::Custom(<fn>)"),
        }
    }
}
