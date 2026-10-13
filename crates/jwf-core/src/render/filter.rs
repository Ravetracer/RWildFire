//! Spatial filter kernels (port of `org.jwildfire.create.tina.render.filter`).

use crate::math::{EPSILON, M_PI, sinc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FilterKernel {
    Bell,
    Blackman,
    Box,
    BSpline,
    CatRom,
    Gaussian,
    Hamming,
    Hanning,
    Hermite,
    Lanczos2,
    Lanczos3,
    Mitchell,
    #[default]
    MitchellSmooth,
    Quadratic,
    SinePow5,
    SinePow10,
    SinePow15,
    Triangle,
    /// Adaptive: Mitchell (smooth) at edges, sine pow 10 in smooth and low-density areas.
    MitchellSinePow,
}

impl FilterKernel {
    pub const ALL: &'static [FilterKernel] = &[
        FilterKernel::Bell,
        FilterKernel::Blackman,
        FilterKernel::Box,
        FilterKernel::BSpline,
        FilterKernel::CatRom,
        FilterKernel::Gaussian,
        FilterKernel::Hamming,
        FilterKernel::Hanning,
        FilterKernel::Hermite,
        FilterKernel::Lanczos2,
        FilterKernel::Lanczos3,
        FilterKernel::Mitchell,
        FilterKernel::MitchellSmooth,
        FilterKernel::Quadratic,
        FilterKernel::SinePow5,
        FilterKernel::SinePow10,
        FilterKernel::SinePow15,
        FilterKernel::Triangle,
        FilterKernel::MitchellSinePow,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            FilterKernel::Bell => "BELL",
            FilterKernel::Blackman => "BLACKMAN",
            FilterKernel::Box => "BOX",
            FilterKernel::BSpline => "BSPLINE",
            FilterKernel::CatRom => "CATROM",
            FilterKernel::Gaussian => "GAUSSIAN",
            FilterKernel::Hamming => "HAMMING",
            FilterKernel::Hanning => "HANNING",
            FilterKernel::Hermite => "HERMITE",
            FilterKernel::Lanczos2 => "LANCZOS2",
            FilterKernel::Lanczos3 => "LANCZOS3",
            FilterKernel::Mitchell => "MITCHELL",
            FilterKernel::MitchellSmooth => "MITCHELL_SMOOTH",
            FilterKernel::Quadratic => "QUADRATIC",
            FilterKernel::SinePow5 => "SINEPOW5",
            FilterKernel::SinePow10 => "SINEPOW10",
            FilterKernel::SinePow15 => "SINEPOW15",
            FilterKernel::Triangle => "TRIANGLE",
            FilterKernel::MitchellSinePow => "MITCHELL_SINEPOW",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            FilterKernel::Bell => "Bell",
            FilterKernel::Blackman => "Blackman",
            FilterKernel::Box => "Box",
            FilterKernel::BSpline => "B-Spline",
            FilterKernel::CatRom => "Catmull-Rom",
            FilterKernel::Gaussian => "Gaussian",
            FilterKernel::Hamming => "Hamming",
            FilterKernel::Hanning => "Hanning",
            FilterKernel::Hermite => "Hermite",
            FilterKernel::Lanczos2 => "Lanczos 2",
            FilterKernel::Lanczos3 => "Lanczos 3",
            FilterKernel::Mitchell => "Mitchell",
            FilterKernel::MitchellSmooth => "Mitchell (smooth)",
            FilterKernel::Quadratic => "Quadratic",
            FilterKernel::SinePow5 => "Sine pow 5",
            FilterKernel::SinePow10 => "Sine pow 10",
            FilterKernel::SinePow15 => "Sine pow 15",
            FilterKernel::Triangle => "Triangle",
            FilterKernel::MitchellSinePow => "Mitchell / sine pow (adaptive)",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|k| k.as_str() == s)
    }

    pub fn is_adaptive(&self) -> bool {
        *self == FilterKernel::MitchellSinePow
    }

    pub fn is_sharpening(&self) -> bool {
        matches!(
            self,
            FilterKernel::Blackman
                | FilterKernel::CatRom
                | FilterKernel::Hamming
                | FilterKernel::Hanning
                | FilterKernel::Lanczos2
                | FilterKernel::Lanczos3
                | FilterKernel::Mitchell
                | FilterKernel::MitchellSmooth
                | FilterKernel::MitchellSinePow
        )
    }

    pub fn spatial_support(&self) -> f64 {
        match self {
            FilterKernel::Bell | FilterKernel::Gaussian | FilterKernel::Quadratic => 1.5,
            FilterKernel::Box => 0.5,
            FilterKernel::BSpline
            | FilterKernel::CatRom
            | FilterKernel::Lanczos2
            | FilterKernel::Mitchell
            | FilterKernel::MitchellSmooth
            | FilterKernel::MitchellSinePow => 2.0,
            FilterKernel::Lanczos3 => 3.0,
            FilterKernel::Blackman
            | FilterKernel::Hamming
            | FilterKernel::Hanning
            | FilterKernel::Hermite
            | FilterKernel::SinePow5
            | FilterKernel::SinePow10
            | FilterKernel::SinePow15
            | FilterKernel::Triangle => 1.0,
        }
    }

    pub fn coeff(&self, t: f64) -> f64 {
        match self {
            FilterKernel::Bell => {
                let t = t.abs();
                if t < 0.5 {
                    0.75 - t * t
                } else if t < 1.5 {
                    let t = t - 1.5;
                    0.5 * t * t
                } else {
                    0.0
                }
            }
            FilterKernel::Blackman => sinc(t) * (0.42 + 0.5 * (M_PI * t).cos() + 0.08 * (2.0 * M_PI * t).cos()),
            FilterKernel::Box => {
                if t > -0.5 && t <= 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            FilterKernel::BSpline => {
                let t = t.abs();
                if t < 1.0 {
                    let tt = t * t;
                    0.5 * tt * t - tt + 2.0 / 3.0
                } else if t < 2.0 {
                    let t = 2.0 - t;
                    (1.0 / 6.0) * t * t * t
                } else {
                    0.0
                }
            }
            FilterKernel::CatRom => {
                let x = t;
                if x < -2.0 {
                    0.0
                } else if x < -1.0 {
                    0.5 * (4.0 + x * (8.0 + x * (5.0 + x)))
                } else if x < 0.0 {
                    0.5 * (2.0 + x * x * (-5.0 - 3.0 * x))
                } else if x < 1.0 {
                    0.5 * (2.0 + x * x * (-5.0 + 3.0 * x))
                } else if x < 2.0 {
                    0.5 * (4.0 + x * (-8.0 + x * (5.0 - x)))
                } else {
                    0.0
                }
            }
            FilterKernel::Gaussian => (-2.0 * t * t).exp() * (2.0 / M_PI).sqrt(),
            FilterKernel::Hamming => sinc(t) * (0.54 + 0.46 * (M_PI * t).cos()),
            FilterKernel::Hanning => sinc(t) * (0.5 + 0.5 * (M_PI * t).cos()),
            FilterKernel::Hermite => {
                let t = t.abs();
                if t < 1.0 { (2.0 * t - 3.0) * t * t + 1.0 } else { 0.0 }
            }
            FilterKernel::Lanczos2 => {
                let t = t.abs();
                if t < 2.0 { sinc(t / 2.0) * (sinc(t) * sinc(t / 2.0)) } else { 0.0 }
            }
            FilterKernel::Lanczos3 => {
                let t = t.abs();
                if t < 3.0 { sinc(t / 3.0) * (sinc(t) * sinc(t / 3.0)) } else { 0.0 }
            }
            FilterKernel::Mitchell => mitchell(t, 1.0 / 3.0, 1.0 / 3.0),
            FilterKernel::MitchellSmooth | FilterKernel::MitchellSinePow => mitchell(t, 0.42, 0.29),
            FilterKernel::Quadratic => {
                let x = t;
                if x < -1.5 {
                    0.0
                } else if x < -0.5 {
                    0.5 * (x + 1.5) * (x + 1.5)
                } else if x < 0.5 {
                    0.75 - x * x
                } else if x < 1.5 {
                    0.5 * (x - 1.5) * (x - 1.5)
                } else {
                    0.0
                }
            }
            FilterKernel::SinePow5 => sinepow(t, 5.0),
            FilterKernel::SinePow10 => sinepow(t, 10.0),
            FilterKernel::SinePow15 => sinepow(t, 15.0),
            FilterKernel::Triangle => {
                let t = t.abs();
                if t < 1.0 { 1.0 - t } else { 0.0 }
            }
        }
    }

    pub fn filter_size(&self, radius: f64, oversample: u32) -> usize {
        if radius < EPSILON {
            return 0;
        }
        let fw = (2.0 * oversample as f64 * self.spatial_support() * radius) as i64;
        let mut size = fw + 1;
        if size % 2 == 0 {
            size += 1;
        }
        size as usize
    }

    fn filter_adjust(&self, radius: f64, oversample: u32) -> f64 {
        if radius < EPSILON {
            return 0.0;
        }
        let fw = (2.0 * oversample as f64 * self.spatial_support() * radius) as i64;
        let mut size = fw + 1;
        if size % 2 == 0 {
            size += 1;
        }
        if fw > 0 { self.spatial_support() * size as f64 / fw as f64 } else { 1.0 }
    }
}

fn mitchell(t: f64, b: f64, c: f64) -> f64 {
    let tt = t * t;
    let t = t.abs();
    if t < 1.0 {
        let v = (12.0 - 9.0 * b - 6.0 * c) * (t * tt) + (-18.0 + 12.0 * b + 6.0 * c) * tt + (6.0 - 2.0 * b);
        v / 6.0
    } else if t < 2.0 {
        let v = (-b - 6.0 * c) * (t * tt) + (6.0 * b + 30.0 * c) * tt + (-12.0 * b - 48.0 * c) * t + (8.0 * b + 24.0 * c);
        v / 6.0
    } else {
        0.0
    }
}

fn sinepow(x: f64, power: f64) -> f64 {
    let res = (2.0 * (power * x.log10()).exp() * 2.0 - 1.0).acos() / M_PI;
    if res.is_finite() { res } else { 0.0 }
}

/// Precomputed, normalized filter matrix (`FilterHolder`).
#[derive(Clone, Debug)]
pub struct FilterMatrix {
    pub size: usize,
    /// `size / 2 - 1`, as in JWildfire (intentionally asymmetric for even halves).
    pub half: i64,
    pub data: Vec<f64>,
    pub kernel: FilterKernel,
    /// Filter radius (`FilterHolder.getIntensity`).
    pub radius: f64,
}

impl FilterMatrix {
    pub fn new(kernel: FilterKernel, oversample: u32, radius: f64) -> Self {
        let size = kernel.filter_size(radius, oversample);
        let half = size as i64 / 2 - 1;
        let mut data = vec![0.0; size * size];
        if size > 0 {
            let adjust = kernel.filter_adjust(radius, oversample);
            let mut sum = 0.0;
            for i in 0..size {
                for j in 0..size {
                    let ii = ((2.0 * i as f64 + 1.0) / size as f64 - 1.0) * adjust;
                    let jj = ((2.0 * j as f64 + 1.0) / size as f64 - 1.0) * adjust;
                    let f = kernel.coeff((ii * ii + jj * jj).sqrt());
                    sum += f;
                    data[i * size + j] = f;
                }
            }
            let os2 = (oversample * oversample) as f64;
            for v in &mut data {
                *v = *v / sum * os2;
            }
        }
        Self { size, half, data, kernel, radius }
    }

    pub fn is_empty(&self) -> bool {
        self.size <= 1
    }

    #[inline]
    pub fn at(&self, i: usize, j: usize) -> f64 {
        self.data[i * self.size + j]
    }
}
