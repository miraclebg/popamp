//! The bar's strip under Xwayland, where the window manager does not keep
//! other windows out of an X11 window's strut: KWin under Wayland lists no
//! `_NET_WM_STRUT_PARTIAL` and maximizes windows under the bar. There the
//! strip is held by a layer of the desktop of the compositor's own
//! (wlr-layer-shell), on a Wayland connection of its own: a transparent
//! line one pixel high at the bar's edge that the mouse goes through, with
//! an exclusive zone as high as the bar. The bar itself stays the X11
//! window it is.

use popamp_core::config::BarEdge;
use std::os::fd::AsFd;
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::protocol::wl_region::WlRegion;
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_shm::{self, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{delegate_noop, Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_manager_v1::ZxdgOutputManagerV1;
use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_v1::{self, ZxdgOutputV1};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::{self, ZwlrLayerShellV1};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{self, ZwlrLayerSurfaceV1};

/// What the compositor tells of its monitors and of the layer.
#[derive(Default)]
struct State {
    /// Each monitor's name and width in the compositor's units, by the
    /// order the outputs were bound in.
    outputs: Vec<(Option<String>, Option<i32>)>,
    /// The layer's width as the compositor configured it, not acked yet.
    configure: Option<(u32, u32)>,
    closed: bool,
}

/// The layer standing in for the bar's strip, and its picture.
struct Layer {
    surface: WlSurface,
    layer: ZwlrLayerSurfaceV1,
    picture: Option<(WlShmPool, WlBuffer)>,
}

impl Drop for Layer {
    fn drop(&mut self) {
        self.layer.destroy();
        self.surface.destroy();
        if let Some((pool, buffer)) = self.picture.take() {
            buffer.destroy();
            pool.destroy();
        }
    }
}

pub struct StripLayer {
    conn: Connection,
    queue: EventQueue<State>,
    state: State,
    compositor: WlCompositor,
    shm: WlShm,
    shell: ZwlrLayerShellV1,
    outputs: Vec<WlOutput>,
    layer: Option<Layer>,
}

impl StripLayer {
    /// Connects to the Wayland compositor the X server runs under; `None`
    /// when there is none or it makes no layers.
    pub fn connect() -> Option<StripLayer> {
        let conn = Connection::connect_to_env().ok()?;
        let (globals, mut queue) = registry_queue_init::<State>(&conn).ok()?;
        let qh = queue.handle();
        let shell: ZwlrLayerShellV1 = globals.bind(&qh, 1..=4, ()).ok()?;
        let compositor: WlCompositor = globals.bind(&qh, 1..=4, ()).ok()?;
        let shm: WlShm = globals.bind(&qh, 1..=1, ()).ok()?;
        let names: Option<ZxdgOutputManagerV1> = globals.bind(&qh, 2..=3, ()).ok();
        let mut state = State::default();
        let outputs: Vec<WlOutput> = globals.contents().with_list(|list| {
            let outputs = list.iter().filter(|g| g.interface == "wl_output");
            outputs.map(|g| globals.registry().bind(g.name, g.version.min(4), &qh, ())).collect()
        });
        for (i, output) in outputs.iter().enumerate() {
            state.outputs.push((None, None));
            if let Some(names) = &names {
                names.get_xdg_output(output, &qh, i);
            }
        }
        queue.roundtrip(&mut state).ok()?;
        Some(StripLayer { conn, queue, state, compositor, shm, shell, outputs, layer: None })
    }

    /// Holds a strip `height` X pixels high at `edge` of the monitor named
    /// `monitor`, `monitor_w` X pixels wide, in place of the one held so far.
    pub fn hold(&mut self, monitor: Option<&str>, monitor_w: i32, edge: BarEdge, height: i32) -> Result<(), String> {
        self.layer = None;
        let i = output_of(&self.state.outputs, monitor).ok_or("the compositor tells of no monitor")?;
        let units = self.state.outputs[i].1.unwrap_or(monitor_w);
        let qh = self.queue.handle();
        let surface = self.compositor.create_surface(&qh, ());
        // The mouse goes through it to what is beneath.
        let region: WlRegion = self.compositor.create_region(&qh, ());
        surface.set_input_region(Some(&region));
        region.destroy();
        let namespace = "popamp-bar-strip".to_string();
        let layer = self.shell.get_layer_surface(&surface, Some(&self.outputs[i]), zwlr_layer_shell_v1::Layer::Bottom, namespace, &qh, ());
        layer.set_anchor(anchor(edge));
        layer.set_size(0, 1);
        layer.set_exclusive_zone(exclusive_zone(height, monitor_w, units));
        surface.commit();
        self.layer = Some(Layer { surface, layer, picture: None });
        self.state.closed = false;
        self.queue.roundtrip(&mut self.state).map_err(|e| e.to_string())?;
        self.answer()
    }

    /// Gives the strip back.
    pub fn release(&mut self) {
        self.layer = None;
        let _ = self.conn.flush();
    }

    /// Answers what the compositor said since, without waiting for it.
    pub fn poll(&mut self) {
        let _ = self.conn.flush();
        if let Some(guard) = self.queue.prepare_read() {
            let _ = guard.read();
        }
        if let Err(e) = self.queue.dispatch_pending(&mut self.state).map_err(|e| e.to_string()).and_then(|_| self.answer()) {
            log::warn!("Wayland strip for the bar: {e}");
        }
    }

    /// Acks a new size with a transparent picture that big.
    fn answer(&mut self) -> Result<(), String> {
        if self.state.closed {
            self.state.closed = false;
            self.layer = None;
            log::warn!("Wayland: the compositor took the bar's strip away");
        }
        let (Some(layer), Some((serial, width))) = (&mut self.layer, self.state.configure.take()) else { return Ok(()) };
        layer.layer.ack_configure(serial);
        let width = width.max(1) as i32;
        let file = transparent_file(width).map_err(|e| format!("no memory for the bar's strip: {e}"))?;
        let qh = self.queue.handle();
        let pool = self.shm.create_pool(file.as_fd(), width * 4, &qh, ());
        let buffer = pool.create_buffer(0, width, 1, width * 4, wl_shm::Format::Argb8888, &qh, ());
        layer.surface.attach(Some(&buffer), 0, 0);
        layer.surface.commit();
        if let Some((pool, buffer)) = layer.picture.replace((pool, buffer)) {
            buffer.destroy();
            pool.destroy();
        }
        self.conn.flush().map_err(|e| e.to_string())
    }
}

/// An unnamed file of `width` transparent pixels.
fn transparent_file(width: i32) -> std::io::Result<std::fs::File> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, Into::into);
    let path = dir.join(format!("popamp-strip-{}-{:016x}", std::process::id(), fastrand::u64(..)));
    let file = std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(&path)?;
    let _ = std::fs::remove_file(&path);
    // A new file reads as zeros: transparent pixels.
    file.set_len(width as u64 * 4)?;
    Ok(file)
}

/// The output named `name`; else the only one there is.
fn output_of(outputs: &[(Option<String>, Option<i32>)], name: Option<&str>) -> Option<usize> {
    let named = name.and_then(|name| outputs.iter().position(|(n, _)| n.as_deref() == Some(name)));
    named.or((outputs.len() == 1).then_some(0))
}

/// `height` X pixels in the units of a monitor `monitor_w` X pixels and
/// `units` compositor units wide, rounded up so the bar is all kept clear.
fn exclusive_zone(height: i32, monitor_w: i32, units: i32) -> i32 {
    if monitor_w <= 0 || units <= 0 {
        return height;
    }
    (height.max(0) as u64 * units as u64).div_ceil(monitor_w as u64) as i32
}

fn anchor(edge: BarEdge) -> zwlr_layer_surface_v1::Anchor {
    use zwlr_layer_surface_v1::Anchor;
    let side = match edge {
        BarEdge::Top => Anchor::Top,
        BarEdge::Bottom => Anchor::Bottom,
    };
    Anchor::Left | Anchor::Right | side
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(_: &mut State, _: &WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<State>) {}
}

impl Dispatch<ZxdgOutputV1, usize> for State {
    fn event(state: &mut State, _: &ZxdgOutputV1, event: zxdg_output_v1::Event, i: &usize, _: &Connection, _: &QueueHandle<State>) {
        let Some(output) = state.outputs.get_mut(*i) else { return };
        match event {
            zxdg_output_v1::Event::Name { name } => output.0 = Some(name),
            zxdg_output_v1::Event::LogicalSize { width, .. } => output.1 = Some(width),
            _ => {}
        }
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, ()> for State {
    fn event(
        state: &mut State,
        _: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, width, .. } => state.configure = Some((serial, width)),
            zwlr_layer_surface_v1::Event::Closed => state.closed = true,
            _ => {}
        }
    }
}

delegate_noop!(State: ignore WlOutput);
delegate_noop!(State: ignore WlShm);
delegate_noop!(State: ignore WlBuffer);
delegate_noop!(State: WlCompositor);
delegate_noop!(State: WlRegion);
delegate_noop!(State: WlShmPool);
delegate_noop!(State: ignore WlSurface);
delegate_noop!(State: ZwlrLayerShellV1);
delegate_noop!(State: ZxdgOutputManagerV1);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_strip_is_as_high_as_the_bar_in_the_compositors_units() {
        // Unscaled, and a 2560-pixel monitor at 200 % (1280 units).
        assert_eq!(exclusive_zone(44, 1280, 1280), 44);
        assert_eq!(exclusive_zone(44, 2560, 1280), 22);
        // 45 pixels at 200 % are 22.5 units: all of the bar is kept clear.
        assert_eq!(exclusive_zone(45, 2560, 1280), 23);
    }

    #[test]
    fn the_strip_goes_on_the_monitor_of_that_name_else_the_only_one() {
        let two = [(Some("DP-1".to_string()), Some(1920)), (Some("HDMI-A-1".to_string()), Some(1280))];
        assert_eq!(output_of(&two, Some("HDMI-A-1")), Some(1));
        assert_eq!(output_of(&two, Some("eDP-1")), None);
        assert_eq!(output_of(&two, None), None);
        let one = [(Some("Virtual-1".to_string()), Some(1280))];
        assert_eq!(output_of(&one, Some("XWAYLAND0")), Some(0));
    }
}
