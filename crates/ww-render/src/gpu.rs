use crate::RenderError;

/// How to pick a graphics adapter.
#[derive(Debug, Clone)]
pub struct GpuOptions {
    /// Prefer the integrated GPU. Wallpapers run all day, so the default is
    /// `true`.
    pub low_power: bool,
    /// Accept a software adapter (WARP, llvmpipe, lavapipe) if no hardware
    /// adapter exists. Used by tests and CI.
    pub allow_software: bool,
    /// Ask for GPU timestamp queries when the adapter has them, for the
    /// benchmark's GPU-time numbers.
    pub timestamps: bool,
    /// Restrict to these backends. `None` uses what the platform offers.
    pub backends: Option<wgpu::Backends>,
}

impl Default for GpuOptions {
    fn default() -> Self {
        GpuOptions {
            low_power: true,
            allow_software: false,
            timestamps: false,
            backends: None,
        }
    }
}

impl GpuOptions {
    /// Options for headless tests: any adapter, software included.
    pub fn for_tests() -> GpuOptions {
        GpuOptions {
            allow_software: true,
            ..GpuOptions::default()
        }
    }
}

/// What adapter is in use, for status displays and benchmark reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterSummary {
    /// Adapter name from the driver.
    pub name: String,
    /// Graphics API, such as `Vulkan` or `Dx12`.
    pub backend: String,
    /// Integrated, discrete, CPU (software) and so on.
    pub device_type: String,
    /// Driver name and version, when reported.
    pub driver: String,
    /// True for software rasterizers.
    pub software: bool,
}

impl std::fmt::Display for AdapterSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({}, {})", self.name, self.backend, self.device_type)?;
        if !self.driver.is_empty() {
            write!(f, ", driver {}", self.driver)?;
        }
        Ok(())
    }
}

/// An open graphics device. Dropping it releases the device and, with the
/// last handle, the driver's memory.
#[derive(Debug)]
pub struct Gpu {
    pub(crate) instance: wgpu::Instance,
    pub(crate) adapter: wgpu::Adapter,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) timestamps: bool,
    summary: AdapterSummary,
}

impl Gpu {
    /// Create the wgpu instance used for adapters and surfaces.
    pub fn instance(opts: &GpuOptions) -> wgpu::Instance {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        if let Some(b) = opts.backends {
            desc.backends = b;
        }
        // Validation layers cost memory and CPU; only use them when a
        // developer asks through WGPU_DEBUG / WGPU_VALIDATION.
        if !cfg!(debug_assertions) {
            desc.flags = wgpu::InstanceFlags::empty().with_env();
        }
        wgpu::Instance::new(desc)
    }

    /// Open a device with no window, for tests, benchmarks and offscreen
    /// rendering.
    pub fn new(opts: &GpuOptions) -> Result<Gpu, RenderError> {
        let instance = Self::instance(opts);
        Self::with_instance(instance, None, opts)
    }

    /// Open a device that can present to `surface`.
    pub fn with_instance(
        instance: wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
        opts: &GpuOptions,
    ) -> Result<Gpu, RenderError> {
        let power = if opts.low_power {
            wgpu::PowerPreference::LowPower
        } else {
            wgpu::PowerPreference::HighPerformance
        };
        let hardware = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: power,
            force_fallback_adapter: false,
            compatible_surface: surface,
            apply_limit_buckets: false,
        }));
        let adapter = match hardware {
            Ok(a) if opts.allow_software || a.get_info().device_type != wgpu::DeviceType::Cpu => a,
            _ if opts.allow_software => {
                pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: power,
                    force_fallback_adapter: true,
                    compatible_surface: surface,
                    apply_limit_buckets: false,
                }))
                .map_err(|e| RenderError::NoAdapter(e.to_string()))?
            }
            Ok(_) => {
                return Err(RenderError::NoAdapter(
                    "only a software adapter is available".into(),
                ));
            }
            Err(e) => return Err(RenderError::NoAdapter(e.to_string())),
        };

        let info = adapter.get_info();
        let timestamps =
            opts.timestamps && adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let mut required_features = wgpu::Features::empty();
        if timestamps {
            required_features |= wgpu::Features::TIMESTAMP_QUERY;
        }
        // Ask for only what we use, so the driver reserves as little as it can.
        let required_limits = wgpu::Limits::downlevel_webgl2_defaults()
            .using_resolution(adapter.limits())
            .using_alignment(adapter.limits());
        let required_limits = wgpu::Limits {
            max_texture_array_layers: adapter.limits().max_texture_array_layers,
            ..required_limits
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("wallweaver"),
            required_features,
            required_limits,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
        }))
        .map_err(|e| RenderError::Device(e.to_string()))?;

        let summary = AdapterSummary {
            name: info.name.clone(),
            backend: format!("{:?}", info.backend),
            device_type: format!("{:?}", info.device_type),
            driver: [info.driver.as_str(), info.driver_info.as_str()]
                .iter()
                .filter(|s| !s.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" "),
            software: info.device_type == wgpu::DeviceType::Cpu,
        };
        Ok(Gpu {
            instance,
            adapter,
            device,
            queue,
            timestamps,
            summary,
        })
    }

    /// The adapter in use.
    pub fn adapter(&self) -> &AdapterSummary {
        &self.summary
    }

    /// True if GPU timestamps are available for frame timing.
    pub fn has_timestamps(&self) -> bool {
        self.timestamps
    }

    /// The wgpu instance, for creating surfaces.
    pub fn wgpu_instance(&self) -> &wgpu::Instance {
        &self.instance
    }

    /// Largest number of layers in one texture array.
    pub(crate) fn max_layers(&self) -> u32 {
        self.device.limits().max_texture_array_layers
    }

    /// Largest texture side.
    pub(crate) fn max_texture_side(&self) -> u32 {
        self.device.limits().max_texture_dimension_2d
    }

    /// Wait until the GPU has finished all submitted work.
    pub fn wait_idle(&self) {
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }
}
