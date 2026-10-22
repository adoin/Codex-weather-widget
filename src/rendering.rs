//! Keep the native Windows renderer away from Intel's unstable OpenGL ICD.
pub fn native_options(viewport: eframe::egui::ViewportBuilder) -> eframe::NativeOptions {
    let mut options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    #[cfg(target_os = "windows")]
    {
        use eframe::egui_wgpu::{WgpuSetup, WgpuSetupCreateNew, wgpu};
        let mut setup = WgpuSetupCreateNew::without_display_handle();
        // Never silently fall back to GL: that reintroduces the observed crash.
        setup.instance_descriptor.backends = wgpu::Backends::DX12;
        // The default HWND swapchain only supports opaque presentation. A
        // DirectComposition visual supports premultiplied alpha on Windows.
        setup
            .instance_descriptor
            .backend_options
            .dx12
            .presentation_system = wgpu::Dx12SwapchainKind::DxgiFromVisual;
        options.renderer = eframe::Renderer::Wgpu;
        options.wgpu_options.wgpu_setup = WgpuSetup::CreateNew(setup);
    }
    #[cfg(not(target_os = "windows"))]
    {
        options.renderer = eframe::Renderer::Glow;
    }
    options
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    #[test]
    fn windows_uses_only_dx12_with_transparent_composition() {
        use eframe::egui_wgpu::{WgpuSetup, wgpu};
        let options = super::native_options(eframe::egui::ViewportBuilder::default());
        assert_eq!(options.renderer, eframe::Renderer::Wgpu);
        let WgpuSetup::CreateNew(setup) = options.wgpu_options.wgpu_setup else {
            panic!("expected a new DX12 device");
        };
        assert_eq!(setup.instance_descriptor.backends, wgpu::Backends::DX12);
        assert_eq!(
            setup
                .instance_descriptor
                .backend_options
                .dx12
                .presentation_system,
            wgpu::Dx12SwapchainKind::DxgiFromVisual
        );
    }
}
