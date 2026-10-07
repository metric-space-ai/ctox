use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoxtralSttBackend {
    Cpu,
    Metal,
    Cuda,
    Wgsl,
}

impl VoxtralSttBackend {
    /// Compiled capability, not model readiness or successful GPU admission.
    pub fn is_available(self) -> bool {
        match self {
            Self::Cpu => crate::GGML_CPU_ENABLED,
            Self::Metal => crate::GGML_METAL_ENABLED,
            Self::Cuda => crate::GGML_CUDA_ENABLED,
            Self::Wgsl => false,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Cpu => "cpu-rust-reference",
            Self::Metal => "metal-vendored-kernels",
            Self::Cuda => "cuda-vendored-kernels",
            Self::Wgsl => "wgsl-vendored-kernels",
        }
    }
}

pub trait KernelBackend {
    fn name(&self) -> &'static str;
    fn argmax(&mut self, x: &[f32]) -> usize;
    fn add_inplace(&mut self, a: &mut [f32], b: &[f32]) -> Result<()>;
    fn silu_inplace(&mut self, x: &mut [f32]) -> Result<()>;
    fn gelu_inplace(&mut self, x: &mut [f32]) -> Result<()>;
}

#[cfg(test)]
mod backend_tests {
    use super::*;

    #[test]
    fn capability_does_not_advertise_uncompiled_gpu_backends() {
        assert_eq!(
            VoxtralSttBackend::Cuda.is_available(),
            crate::GGML_CUDA_ENABLED
        );
        assert_eq!(
            VoxtralSttBackend::Metal.is_available(),
            crate::GGML_METAL_ENABLED
        );
        assert!(!VoxtralSttBackend::Wgsl.is_available());
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CpuBackend;

impl KernelBackend for CpuBackend {
    fn name(&self) -> &'static str {
        "cpu-rust-reference"
    }

    fn argmax(&mut self, x: &[f32]) -> usize {
        x.iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(idx, _)| idx)
            .unwrap_or(0)
    }

    fn add_inplace(&mut self, a: &mut [f32], b: &[f32]) -> Result<()> {
        for (lhs, rhs) in a.iter_mut().zip(b.iter().copied()) {
            *lhs += rhs;
        }
        Ok(())
    }

    fn silu_inplace(&mut self, x: &mut [f32]) -> Result<()> {
        for value in x {
            *value = *value / (1.0 + (-*value).exp());
        }
        Ok(())
    }

    fn gelu_inplace(&mut self, x: &mut [f32]) -> Result<()> {
        for value in x {
            *value *= 0.5 * (1.0 + Erf::erf(*value / std::f32::consts::SQRT_2));
        }
        Ok(())
    }
}

trait Erf {
    fn erf(self) -> Self;
}

impl Erf for f32 {
    fn erf(self) -> Self {
        // Abramowitz and Stegun 7.1.26. Adequate for CPU reference activations.
        let sign = if self < 0.0 { -1.0 } else { 1.0 };
        let x = self.abs();
        let t = 1.0 / (1.0 + 0.3275911 * x);
        let y = 1.0
            - (((((1.0614054 * t - 1.4531521) * t) + 1.4214138) * t - 0.28449672) * t + 0.2548296)
                * t
                * (-x * x).exp();
        sign * y
    }
}
