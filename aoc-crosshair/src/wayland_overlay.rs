//! Overlay Wayland via `zwlr_layer_shell_v1` (KWin / wlroots).

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use aoc_gmenu::config::CrosshairConfig;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
    shm::{
        slot::{Buffer, SlotPool},
        Shm, ShmHandler,
    },
};
use wayland_client::{
    delegate_noop,
    globals::registry_queue_init,
    protocol::{wl_output, wl_region::WlRegion, wl_shm, wl_surface},
    Connection, QueueHandle,
};

use aoc_crosshair::render::{pixmap_size, render_crosshair};

pub struct OverlayApp {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    #[allow(dead_code)]
    compositor: CompositorState,

    pool: SlotPool,
    layer: LayerSurface,
    width: u32,
    height: u32,
    first_configure: bool,
    exit_flag: Arc<AtomicBool>,
    reload_flag: Arc<AtomicBool>,
    cfg: CrosshairConfig,
    cfg_path: std::path::PathBuf,
    last_mtime: Option<std::time::SystemTime>,
    last_poll: Instant,
    _buffer: Option<Buffer>,
    wanted_output: String,
}

impl OverlayApp {
    pub fn run(
        cfg: CrosshairConfig,
        cfg_path: std::path::PathBuf,
        exit_flag: Arc<AtomicBool>,
        reload_flag: Arc<AtomicBool>,
    ) -> Result<(), String> {
        let conn = Connection::connect_to_env().map_err(|e| format!("wayland connect: {e}"))?;
        let (globals, mut event_queue) =
            registry_queue_init(&conn).map_err(|e| format!("registry: {e}"))?;
        let qh = event_queue.handle();

        let compositor = CompositorState::bind(&globals, &qh)
            .map_err(|_| "wl_compositor indisponível".to_string())?;
        let layer_shell = LayerShell::bind(&globals, &qh)
            .map_err(|_| "zwlr_layer_shell_v1 indisponível (KWin/wlroots?)".to_string())?;
        let shm = Shm::bind(&globals, &qh).map_err(|_| "wl_shm indisponível".to_string())?;

        let output_state = OutputState::new(&globals, &qh);
        let surface = compositor.create_surface(&qh);
        let dim = pixmap_size(&cfg);

        // Tenta associar ao output nomeado (DP-1 etc.).
        let target = find_output(&output_state, &cfg.output);
        let layer = layer_shell.create_layer_surface(
            &qh,
            surface,
            Layer::Overlay,
            Some("aoc-crosshair"),
            target.as_ref(),
        );
        // Sem âncoras → compositor centra; exclusive_zone -1.
        layer.set_anchor(Anchor::empty());
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_size(dim, dim);

        // Click-through: região de input vazia.
        let region = compositor.wl_compositor().create_region(&qh, ());
        layer.wl_surface().set_input_region(Some(&region));
        region.destroy();

        layer.commit();

        let pool = SlotPool::new((dim * dim * 4) as usize, &shm)
            .map_err(|e| format!("SlotPool: {e}"))?;

        let last_mtime = std::fs::metadata(&cfg_path)
            .and_then(|m| m.modified())
            .ok();

        let mut app = OverlayApp {
            registry_state: RegistryState::new(&globals),
            output_state,
            shm,
            compositor,
            pool,
            layer,
            width: dim,
            height: dim,
            first_configure: true,
            exit_flag,
            reload_flag,
            wanted_output: cfg.output.clone(),
            cfg,
            cfg_path,
            last_mtime,
            last_poll: Instant::now(),
            _buffer: None,
        };

        // Roundtrip inicial para configure.
        event_queue
            .roundtrip(&mut app)
            .map_err(|e| format!("roundtrip: {e}"))?;

        while !app.exit_flag.load(Ordering::Relaxed) {
            let _ = event_queue.dispatch_pending(&mut app);
            let _ = conn.flush();
            if let Some(guard) = conn.prepare_read() {
                // Poll com timeout via sleep + nonblocking attempt.
                std::thread::sleep(Duration::from_millis(50));
                let _ = guard.read();
            } else {
                std::thread::sleep(Duration::from_millis(50));
            }
            let _ = event_queue.dispatch_pending(&mut app);

            if app.reload_flag.swap(false, Ordering::Relaxed) {
                app.reload_from_disk(&qh);
            }
            app.poll_config(&qh);
        }
        Ok(())
    }

    fn reload_from_disk(&mut self, qh: &QueueHandle<Self>) {
        if let Ok(cfg) = aoc_gmenu::config::load_config_from(&self.cfg_path) {
            if !cfg.crosshair.enabled {
                eprintln!("aoc-crosshair: disabled in config — exiting");
                self.exit_flag.store(true, Ordering::Relaxed);
                return;
            }
            self.apply_cfg(cfg.crosshair, qh);
        }
    }

    fn apply_cfg(&mut self, cfg: CrosshairConfig, qh: &QueueHandle<Self>) {
        self.cfg = cfg;
        self.wanted_output = self.cfg.output.clone();
        let dim = pixmap_size(&self.cfg);
        self.width = dim;
        self.height = dim;
        self.layer.set_size(dim, dim);
        let region = self.compositor.wl_compositor().create_region(qh, ());
        self.layer.wl_surface().set_input_region(Some(&region));
        region.destroy();
        self.layer.commit();
        self.draw(qh);
    }

    fn poll_config(&mut self, qh: &QueueHandle<Self>) {
        if self.last_poll.elapsed() < Duration::from_millis(500) {
            return;
        }
        self.last_poll = Instant::now();
        let Ok(meta) = std::fs::metadata(&self.cfg_path) else {
            return;
        };
        let Ok(mtime) = meta.modified() else {
            return;
        };
        if self.last_mtime == Some(mtime) {
            return;
        }
        self.last_mtime = Some(mtime);
        self.reload_from_disk(qh);
    }

    fn draw(&mut self, _qh: &QueueHandle<Self>) {
        let pixmap = render_crosshair(&self.cfg);
        let width = pixmap.width();
        let height = pixmap.height();
        self.width = width;
        self.height = height;
        let stride = width as i32 * 4;

        let (buffer, canvas) = match self.pool.create_buffer(
            width as i32,
            height as i32,
            stride,
            wl_shm::Format::Argb8888,
        ) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("aoc-crosshair: buffer: {e}");
                return;
            }
        };

        for (src, dst) in pixmap.pixels().iter().zip(canvas.as_chunks_mut::<4>().0) {
            let r = src.red();
            let g = src.green();
            let b = src.blue();
            let a = src.alpha();
            let pr = ((u16::from(r) * u16::from(a)) / 255) as u8;
            let pg = ((u16::from(g) * u16::from(a)) / 255) as u8;
            let pb = ((u16::from(b) * u16::from(a)) / 255) as u8;
            dst[0] = pb;
            dst[1] = pg;
            dst[2] = pr;
            dst[3] = a;
        }

        self.layer
            .wl_surface()
            .damage_buffer(0, 0, width as i32, height as i32);
        let _ = buffer.attach_to(self.layer.wl_surface());
        self.layer.wl_surface().commit();
        self._buffer = Some(buffer);
    }
}

fn find_output(output_state: &OutputState, wanted: &str) -> Option<wl_output::WlOutput> {
    for output in output_state.outputs() {
        if let Some(info) = output_state.info(&output) {
            let name = info.name.unwrap_or_default();
            if name == wanted || name.eq_ignore_ascii_case(wanted) {
                eprintln!("aoc-crosshair: usando output '{name}'");
                return Some(output);
            }
        }
    }
    eprintln!("aoc-crosshair: output '{wanted}' não encontrado; default do compositor");
    None
}

impl CompositorHandler for OverlayApp {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        self.draw(qh);
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for OverlayApp {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for OverlayApp {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.exit_flag.store(true, Ordering::Relaxed);
    }

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        if let Some(w) = NonZeroU32::new(configure.new_size.0) {
            self.width = w.get();
        }
        if let Some(h) = NonZeroU32::new(configure.new_size.1) {
            self.height = h.get();
        }
        if self.first_configure {
            self.first_configure = false;
            self.draw(qh);
        }
    }
}

impl ShmHandler for OverlayApp {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_compositor!(OverlayApp);
delegate_output!(OverlayApp);
delegate_shm!(OverlayApp);
delegate_layer!(OverlayApp);
delegate_registry!(OverlayApp);
delegate_noop!(OverlayApp: ignore WlRegion);

impl ProvidesRegistryState for OverlayApp {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}
