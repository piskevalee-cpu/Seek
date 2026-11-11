//! The vitals of the machine (SPEC 8).
//!
//! CPU, RAM and, where the driver allows it, GPU and VRAM, sampled at 1 Hz. A sensor that is
//! not there shows a dash.

use std::time::Duration;

/// How often the vitals are sampled.
pub const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

/// The vitals of the machine.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Vitals {
    /// The load of the CPU, in percent.
    pub cpu: Option<f32>,
    /// The used memory, in bytes.
    pub ram_used: Option<u64>,
    /// The total memory, in bytes.
    pub ram_total: Option<u64>,
    /// The load of the GPU, in percent.
    pub gpu: Option<f32>,
    /// The used video memory, in bytes.
    pub vram_used: Option<u64>,
    /// The total video memory, in bytes.
    pub vram_total: Option<u64>,
}

impl Vitals {
    /// The CPU, as it is shown.
    pub fn cpu_text(&self) -> String {
        match self.cpu {
            Some(percent) => format!("{percent:.0}%"),
            None => "-".to_string(),
        }
    }

    /// The memory, as it is shown.
    pub fn ram_text(&self) -> String {
        match (self.ram_used, self.ram_total) {
            (Some(used), Some(total)) => {
                format!("{}/{}", format_bytes(used), format_bytes(total))
            }
            _ => "-".to_string(),
        }
    }

    /// The GPU, as it is shown.
    pub fn gpu_text(&self) -> String {
        match self.gpu {
            Some(percent) => format!("{percent:.0}%"),
            None => "-".to_string(),
        }
    }

    /// The video memory, as it is shown.
    pub fn vram_text(&self) -> String {
        match (self.vram_used, self.vram_total) {
            (Some(used), Some(total)) => {
                format!("{}/{}", format_bytes(used), format_bytes(total))
            }
            _ => "-".to_string(),
        }
    }
}

/// Format a number of bytes: `4.2G`.
pub fn format_bytes(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.1}G", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.0}M", bytes / MIB)
    } else {
        format!("{bytes:.0}")
    }
}

/// Samples the vitals of the machine.
#[derive(Debug)]
pub struct VitalsSampler {
    system: Option<sysinfo::System>,
    #[cfg(feature = "nvml")]
    gpu: Option<Gpu>,
    last: Option<std::time::Instant>,
}

/// The NVIDIA GPU, when the driver is there.
#[cfg(feature = "nvml")]
#[derive(Debug)]
struct Gpu {
    _nvml: nvml_wrapper::Nvml,
    index: u32,
}

#[cfg(feature = "nvml")]
impl Gpu {
    /// Open the GPU, if there is one.
    fn open() -> Option<Self> {
        let nvml = nvml_wrapper::Nvml::init().ok()?;
        let count = nvml.device_count().ok()?;
        if count == 0 {
            return None;
        }
        Some(Gpu {
            _nvml: nvml,
            index: 0,
        })
    }

    /// The load and the memory of the GPU.
    fn sample(&self) -> (Option<f32>, Option<u64>, Option<u64>) {
        let Ok(device) = self._nvml.device_by_index(self.index) else {
            return (None, None, None);
        };
        let utilization = device
            .utilization()
            .map(|utilization| utilization.gpu as f32)
            .ok();
        let memory = device
            .memory_info()
            .ok()
            .map(|memory| (memory.used, memory.total));
        (
            utilization,
            memory.map(|memory| memory.0),
            memory.map(|memory| memory.1),
        )
    }
}

impl VitalsSampler {
    /// A sampler.
    pub fn new() -> Self {
        let mut system = sysinfo::System::new().ok();
        if let Some(system) = system.as_mut() {
            system.refresh_cpu_usage();
            system.refresh_memory();
        }
        VitalsSampler {
            system,
            #[cfg(feature = "nvml")]
            gpu: Gpu::open(),
            last: None,
        }
    }

    /// Whether the sampler is due.
    pub fn is_due(&self) -> bool {
        self.last
            .is_none_or(|last| last.elapsed() >= SAMPLE_INTERVAL)
    }

    /// Take a sample.
    pub fn sample(&mut self) -> Vitals {
        self.last = Some(std::time::Instant::now());

        let Some(system) = self.system.as_mut() else {
            return Vitals::default();
        };
        system.refresh_cpu_usage();
        system.refresh_memory();

        #[cfg(feature = "nvml")]
        let (gpu, vram_used, vram_total) = match &self.gpu {
            Some(gpu) => gpu.sample(),
            None => (None, None, None),
        };
        #[cfg(not(feature = "nvml"))]
        let (gpu, vram_used, vram_total) = (None, None, None);

        Vitals {
            cpu: Some(system.global_cpu_usage()),
            ram_used: Some(system.used_memory()),
            ram_total: Some(system.total_memory()),
            gpu,
            vram_used,
            vram_total,
        }
    }
}

impl Default for VitalsSampler {
    fn default() -> Self {
        VitalsSampler::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_sensor_shows_a_dash() {
        let vitals = Vitals::default();
        assert_eq!(vitals.cpu_text(), "-");
        assert_eq!(vitals.ram_text(), "-");
        assert_eq!(vitals.gpu_text(), "-");
        assert_eq!(vitals.vram_text(), "-");
    }

    #[test]
    fn the_values_are_readable() {
        let vitals = Vitals {
            cpu: Some(42.0),
            ram_used: Some(2 * 1024 * 1024 * 1024),
            ram_total: Some(8 * 1024 * 1024 * 1024),
            gpu: Some(7.0),
            vram_used: Some(1024 * 1024 * 1024),
            vram_total: Some(8 * 1024 * 1024 * 1024),
        };
        assert_eq!(vitals.cpu_text(), "42%");
        assert_eq!(vitals.ram_text(), "2.0G/8.0G");
        assert_eq!(vitals.gpu_text(), "7%");
    }

    #[test]
    fn the_sampler_reads_the_machine() {
        let mut sampler = VitalsSampler::new();
        let vitals = sampler.sample();
        assert!(vitals.cpu.is_some(), "the CPU is there");
        assert!(vitals.ram_total.unwrap_or(0) > 0, "the memory is there");
    }

    #[test]
    fn the_bytes_are_formatted() {
        assert_eq!(format_bytes(512), "512");
        assert_eq!(format_bytes(1024 * 1024), "1M");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.0G");
    }
}
