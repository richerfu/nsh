//! One compositor worker per surface; exports and replies are generation scoped.
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

use nsh_core::{Filter, NodeId, animation::AnimationValue, shader::ShaderLayer};
use ohos_native_window_binding::NativeWindow;

use super::{
    raster::{Scene as ScenePlan, Shared as SceneShared},
    readback::{RgbaFrame, TextureContext, TextureReader},
};
use crate::{
    app::{
        frame_rate::FrameRate,
        runtime::{Dispatcher, Message},
    },
    render::{
        composition::filter::Image,
        node::{Node, Result},
    },
};

const MAX_LAYERS: usize = 128;
const MAX_SURFACE_PIXELS: u64 = 67_108_864;

#[derive(Clone, PartialEq)]
struct Settings {
    gpu: bool,
    width: u32,
    height: u32,
    filters: Vec<Filter>,
    scale: f32,
    insets: [u32; 4],
    scene: bool,
    shaders: Vec<(NodeId, Arc<ShaderLayer>)>,
}
impl Settings {
    fn pixels(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }
}
struct Output {
    frame: RgbaFrame,
    x: f32,
    y: f32,
}
struct State {
    settings: Settings,
    screen_origin: Option<(i32, i32)>,
    output: Option<Output>,
    surface: Option<std::result::Result<u64, String>>,
    closed: bool,
}
struct Shared {
    released: AtomicBool,
    active: AtomicBool,
    state: Mutex<State>,
    queued: AtomicBool,
    posted: AtomicBool,
    animating: AtomicBool,
}
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Key {
    pub(crate) id: NodeId,
    pub(crate) generation: u32,
    overlay: bool,
}
impl Key {
    fn slot(self) -> usize {
        self.id as usize * 2 + usize::from(self.overlay)
    }
}
enum Task {
    Create { key: Key, shared: Arc<Shared> },
    Fill(super::fill::Command),
    Frame,
    Resume,
    Wake(Key),
    Slot(Key, NodeId, u16, AnimationValue),
    Transition(Key, NodeId, std::time::Duration),
    Navigation(Navigation),
    Window(Key, NativeWindow),
    WindowGone(Key, usize),
    Plan,
    Close,
}
struct Navigation {
    target: Key,
    key: crate::render::shader::navigation::Key,
    snapshot: Option<super::snapshot::Snapshot>,
    duration: std::time::Duration,
    backwards: bool,
    reverse_inputs: bool,
    complete: bool,
}
pub(crate) struct Worker {
    send: mpsc::Sender<Task>,
    plan: Arc<Mutex<Option<ScenePlan>>>,
    thread: Option<std::thread::JoinHandle<()>>,
    retired: RefCell<Vec<(Arc<Shared>, super::presentation::Presentation)>>,
    retired_fills: Arc<Mutex<Vec<super::fill::Key>>>,
}
#[derive(Clone, Copy)]
enum WindowTarget {
    Composite(Key),
    Fill(super::fill::Key),
}
#[derive(Clone)]
pub(crate) struct WindowEvents {
    target: WindowTarget,
    send: mpsc::Sender<Task>,
}
impl WindowEvents {
    pub(super) fn created(&self, window: NativeWindow) {
        let task = match self.target {
            WindowTarget::Composite(key) => Task::Window(key, window),
            WindowTarget::Fill(key) => Task::Fill(super::fill::Command::Window(key, window)),
        };
        let _ = self.send.send(task);
    }
    pub(super) fn destroyed(&self, raw: usize) {
        let task = match self.target {
            WindowTarget::Composite(key) => Task::WindowGone(key, raw),
            WindowTarget::Fill(key) => Task::Fill(super::fill::Command::WindowGone(key, raw)),
        };
        let _ = self.send.send(task);
    }
}
impl Worker {
    pub(crate) fn for_owner(owner: &Node) -> Result<Rc<Self>> {
        let mut slot = owner.compositor.borrow_mut();
        if slot.is_none() {
            *slot = Some(Rc::new(
                Self::new(owner.dispatcher.clone(), owner.shaders.frame_rate()).map_err(
                    |error| {
                        napi_ohos::Error::from_reason(format!(
                            "Could not start OHOS compositor: {error}"
                        ))
                    },
                )?,
            ));
        }
        Ok(slot.as_ref().unwrap().clone())
    }
    pub(crate) fn fill(&self, command: super::fill::Command) {
        let _ = self.send.send(Task::Fill(command));
    }
    pub(crate) fn fill_window(&self, key: super::fill::Key) -> WindowEvents {
        WindowEvents {
            target: WindowTarget::Fill(key),
            send: self.send.clone(),
        }
    }
    pub(crate) fn take_retired_fills(&self) -> Vec<super::fill::Key> {
        std::mem::take(&mut *self.retired_fills.lock().unwrap())
    }
    pub(crate) fn new(route: Weak<Dispatcher>, rate: Arc<FrameRate>) -> std::io::Result<Self> {
        let (send, receive) = mpsc::channel();
        let callback = send.clone();
        let plan = Arc::new(Mutex::new(None));
        let pending_plan = plan.clone();
        let retired_fills = Arc::new(Mutex::new(Vec::new()));
        let completed_fills = retired_fills.clone();
        let thread = std::thread::Builder::new()
            .name("nsh-compositor".into())
            .spawn(move || {
                let vsync = rate.vsync();
                let mut vote = rate.vote();
                let mut entries: Vec<Option<Entry>> = Vec::new();
                let mut gpu: Option<TextureContext> = None;
                let mut scene: Option<ScenePlan> = None;
                let mut fills = super::fill::Fills::default();
                let mut frame_pending = false;
                while let Ok(task) = receive.recv() {
                    let mut repaint = false;
                    let mut fill_frame = false;
                    match task {
                        Task::Close => break,
                        Task::Fill(super::fill::Command::Remove(key)) => {
                            fills.remove(key);
                            completed_fills.lock().unwrap().push(key);
                            if let Some(route) = route.upgrade() {
                                route.send(Message::Shader);
                            }
                        }
                        Task::Fill(command) => {
                            let result = (|| -> std::result::Result<(), String> {
                                if gpu.is_none() {
                                    gpu = Some(TextureContext::new().map_err(|e| e.to_string())?);
                                }
                                fills.apply(command, gpu.as_ref().unwrap())
                            })();
                            if let Err(error) = result {
                                log::warn!("shader fill: {error}");
                            }
                            // Apply every update, but present only on VSync. A
                            // batch can change several animated uniforms; drawing
                            // all fills after each command starves later exports.
                        }
                        Task::Resume => {
                            fill_frame = true;
                            if route
                                .upgrade()
                                .is_some_and(|r| r.active.load(Ordering::Acquire))
                            {
                                for entry in entries.iter().flatten() {
                                    wake(&callback, entry.key, &entry.shared);
                                }
                            }
                        }
                        Task::Frame => {
                            fill_frame = true;
                            frame_pending = false;
                            for entry in entries.iter().flatten() {
                                if route
                                    .upgrade()
                                    .is_some_and(|r| r.active.load(Ordering::Acquire))
                                    && entry.shared.active.load(Ordering::Acquire)
                                    && entry.shared.animating.load(Ordering::Acquire)
                                {
                                    wake(&callback, entry.key, &entry.shared);
                                }
                            }
                        }
                        Task::Plan => {
                            scene = pending_plan.lock().unwrap().take();
                            repaint = scene.is_some();
                        }
                        Task::Create { key, shared } => {
                            let result = (|| {
                                if shared.state.lock().unwrap().closed {
                                    return Err("composite retired".into());
                                }
                                if entries.iter().flatten().count() >= MAX_LAYERS {
                                    return Err("compositor layer budget exceeded".into());
                                }
                                let settings = shared.state.lock().unwrap().settings.clone();
                                check_pixels(&entries, key, settings.pixels())?;
                                let callback = callback.clone();
                                let weak = Arc::downgrade(&shared);
                                if gpu.is_none() {
                                    gpu = Some(
                                        TextureContext::new().map_err(|error| error.to_string())?,
                                    );
                                }
                                let mut reader =
                                    TextureReader::with_context(gpu.as_ref().unwrap(), move || {
                                        if let Some(shared) = weak.upgrade() {
                                            wake(&callback, key, &shared);
                                        }
                                    })
                                    .map_err(|error| error.to_string())?;
                                reader
                                    .resize(settings.width, settings.height)
                                    .map_err(|error| error.to_string())?;
                                let id = reader.id().map_err(|error| error.to_string())?;
                                entries.resize_with(entries.len().max(key.slot() + 1), || None);
                                entries[key.slot()] = Some(Entry {
                                    key,
                                    shared: shared.clone(),
                                    reader,
                                    settings,
                                    scene_source: None,
                                    shaders: super::gles::Effects::new(gpu.as_ref().unwrap()),
                                    filters: super::gles::Filters::new(gpu.as_ref().unwrap()),
                                    window: None,
                                    window_dirty: false,
                                    navigation: None,
                                    completed: None,
                                });
                                Ok(id)
                            })();
                            if result.is_err() {
                                shared.released.store(true, Ordering::Release);
                            }
                            shared.state.lock().unwrap().surface = Some(result);
                            if let Some(route) = route.upgrade() {
                                route.send(Message::Composite {
                                    id: key.id,
                                    generation: key.generation,
                                });
                            }
                        }
                        Task::Wake(key) => {
                            let Some(entry) = entries
                                .get(key.slot())
                                .and_then(Option::as_ref)
                                .filter(|entry| entry.key == key)
                            else {
                                continue;
                            };
                            entry.shared.queued.store(false, Ordering::Release);
                            if entry.shared.state.lock().unwrap().closed {
                                let shared = entry.shared.clone();
                                entries[key.slot()].take();
                                shared.released.store(true, Ordering::Release);
                                if let Some(route) = route.upgrade() {
                                    route.send(Message::Composite {
                                        id: key.id,
                                        generation: key.generation,
                                    });
                                }
                                continue;
                            }
                            if !route
                                .upgrade()
                                .is_some_and(|r| r.active.load(Ordering::Acquire))
                            {
                                vote.update(false);
                                continue;
                            }
                            let settings = entry.shared.state.lock().unwrap().settings.clone();
                            let result =
                                check_pixels(&entries, key, settings.pixels()).and_then(|()| {
                                    entries[key.slot()].as_mut().unwrap().render(settings)
                                });
                            let Some(route) = route.upgrade() else {
                                break;
                            };
                            match result {
                                Ok(changed) => repaint = changed,
                                Err(error) => {
                                    let entry = entries[key.slot()].as_mut().unwrap();
                                    entry.shared.animating.store(false, Ordering::Release);
                                    if let Some(navigation) = entry.navigation.take() {
                                        route.send(Message::Navigation(navigation.key));
                                    }
                                    route.send(Message::Error(format!("compositor: {error}")));
                                    continue;
                                }
                            }
                            let shared = &entries[key.slot()].as_ref().unwrap().shared;
                            let publish = {
                                let state = shared.state.lock().unwrap();
                                !state.settings.scene && state.output.is_some()
                            };
                            if publish && !shared.posted.swap(true, Ordering::AcqRel) {
                                route.send(Message::Composite {
                                    id: key.id,
                                    generation: key.generation,
                                });
                            }
                            if let Some(key) =
                                entries[key.slot()].as_mut().unwrap().completed.take()
                            {
                                route.send(Message::Navigation(key));
                            }
                        }
                        Task::Slot(key, id, slot, value) => {
                            if let Some(entry) = entries
                                .get_mut(key.slot())
                                .and_then(Option::as_mut)
                                .filter(|entry| entry.key == key)
                            {
                                if let Err(error) = entry
                                    .shaders
                                    .layers(&entry.shared.state.lock().unwrap().settings.shaders)
                                {
                                    log::warn!("shader layers: {error}");
                                }
                                entry.shaders.slot(id, slot, value);
                                wake(&callback, key, &entry.shared);
                            }
                        }
                        Task::Transition(key, id, duration) => {
                            if let Some(entry) = entries
                                .get_mut(key.slot())
                                .and_then(Option::as_mut)
                                .filter(|entry| entry.key == key)
                            {
                                if let Err(error) = entry.shaders.transition(id, duration) {
                                    log::warn!("shader transition: {error}");
                                }
                                wake(&callback, key, &entry.shared);
                            }
                        }
                        Task::Navigation(request) => {
                            if let Some(entry) = entries
                                .get_mut(request.target.slot())
                                .and_then(Option::as_mut)
                                .filter(|e| e.key == request.target)
                            {
                                entry.navigation = Some(request);
                                wake(&callback, entry.key, &entry.shared);
                            }
                        }
                        Task::Window(key, window) => {
                            if let Some(entry) = entries
                                .get_mut(key.slot())
                                .and_then(Option::as_mut)
                                .filter(|e| e.key == key)
                            {
                                match gpu.as_ref().unwrap().window(window) {
                                    Ok(window) => {
                                        entry.window = Some(window);
                                        entry.window_dirty = true;
                                    }
                                    Err(error) => log::warn!("shader window: {error}"),
                                }
                                wake(&callback, key, &entry.shared);
                            }
                        }
                        Task::WindowGone(key, raw) => {
                            if let Some(entry) = entries
                                .get_mut(key.slot())
                                .and_then(Option::as_mut)
                                .filter(|e| e.key == key)
                                && entry.window.as_ref().is_some_and(|w| w.raw() == raw)
                            {
                                entry.window = None;
                            }
                        }
                    }
                    if fill_frame
                        && route
                            .upgrade()
                            .is_some_and(|r| r.active.load(Ordering::Acquire))
                    {
                        fills.render();
                    }
                    // Shader clocks advance on the window's native VSync, just
                    // like fill shaders. UI callbacks only update ownership and
                    // uniforms; they must not divide the GPU presentation rate.
                    let animating = route
                        .upgrade()
                        .is_some_and(|r| r.active.load(Ordering::Acquire))
                        && (fills.animating()
                            || entries.iter().flatten().any(|entry| {
                                entry.window.is_some()
                                    && entry.shared.active.load(Ordering::Acquire)
                                    && entry.shared.animating.load(Ordering::Acquire)
                            }));
                    vote.update(animating);
                    if !frame_pending && animating {
                        let send = callback.clone();
                        frame_pending = vsync.request_frame_once(move |_| {
                            let _ = send.send(Task::Frame);
                        }) == 0;
                    }
                    if repaint && let Some(scene) = &scene {
                        let Some(route) = route.upgrade() else {
                            break;
                        };
                        scene.render(
                            |key| {
                                entries
                                    .get(key.slot())
                                    .and_then(Option::as_ref)
                                    .filter(|entry| entry.key == key)
                                    .and_then(|entry| entry.scene_source.as_ref())
                            },
                            &route,
                        );
                    }
                }
            })?;
        Ok(Self {
            send,
            plan,
            thread: Some(thread),
            retired: RefCell::default(),
            retired_fills,
        })
    }
    pub(crate) fn wake(&self) {
        let _ = self.send.send(Task::Resume);
    }
    pub(crate) fn collect(&self) {
        self.retired
            .borrow_mut()
            .retain(|(shared, _)| !shared.released.load(Ordering::Acquire));
    }
    pub(crate) fn clear_plan(&self) {
        self.plan.lock().unwrap().take();
        let _ = self.send.send(Task::Plan);
    }
    pub(crate) fn plan(
        &self,
        plan: Arc<crate::render::composition::draw::Plan>,
        shared: Arc<SceneShared>,
    ) {
        let mut pending = self.plan.lock().unwrap();
        let wake = pending.is_none();
        *pending = Some(ScenePlan { plan, shared });
        if wake {
            let _ = self.send.send(Task::Plan);
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.send.send(Task::Close);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
fn wake(send: &mpsc::Sender<Task>, key: Key, shared: &Shared) {
    if !shared.queued.swap(true, Ordering::AcqRel) {
        let _ = send.send(Task::Wake(key));
    }
}
fn check_pixels(
    entries: &[Option<Entry>],
    key: Key,
    pixels: u64,
) -> std::result::Result<(), String> {
    if pixels > 16_777_216
        || entries
            .iter()
            .flatten()
            .filter(|entry| entry.key.slot() != key.slot())
            .map(|entry| entry.settings.pixels())
            .sum::<u64>()
            + pixels
            > MAX_SURFACE_PIXELS
    {
        Err("compositor pixel budget exceeded".into())
    } else {
        Ok(())
    }
}
struct Entry {
    key: Key,
    shared: Arc<Shared>,
    reader: TextureReader,
    settings: Settings,
    scene_source: Option<Image>,
    shaders: super::gles::Effects,
    filters: super::gles::Filters,
    window: Option<super::readback::Window>,
    window_dirty: bool,
    navigation: Option<Navigation>,
    completed: Option<crate::render::shader::navigation::Key>,
}
impl Entry {
    fn render(&mut self, settings: Settings) -> std::result::Result<bool, String> {
        if let Some(navigation) = &self.navigation {
            if !settings
                .shaders
                .iter()
                .any(|(id, _)| *id == navigation.key.owner)
            {
                self.navigation = None;
            } else if navigation.complete {
                // Keep the terminal picture until its generation is retired by the UI.
                return Ok(false);
            }
        }
        let mut changed = self.settings != settings || std::mem::take(&mut self.window_dirty);
        self.shaders.layers(&settings.shaders)?;
        if (settings.width, settings.height) != (self.settings.width, self.settings.height) {
            self.reader
                .resize(settings.width, settings.height)
                .map_err(|e| e.to_string())?;
        }
        self.settings = settings;
        let direct = !self.settings.scene;
        for _ in 0..4 {
            if direct {
                if !self.reader.update_texture().map_err(|e| e.to_string())? {
                    break;
                }
            } else {
                let Some(frame) = self.reader.read().map_err(|e| e.to_string())? else {
                    break;
                };
                let mut image = Image::new(frame);
                image.x = -(self.settings.insets[0] as f32);
                image.y = -(self.settings.insets[1] as f32);
                self.scene_source = Some(image);
            }
            changed = true;
        }
        if let Some(navigation) = &mut self.navigation
            && navigation.snapshot.is_some()
        {
            let origin = self.shared.state.lock().unwrap().screen_origin;
            let Some(origin) =
                origin.filter(|_| self.reader.texture().is_some() && self.window.is_some())
            else {
                return Ok(false);
            };
            let mut snapshot = navigation.snapshot.take().unwrap();
            let frame = snapshot.take_aligned(
                self.settings.width,
                self.settings.height,
                (
                    origin.0 - self.settings.insets[0] as i32,
                    origin.1 - self.settings.insets[1] as i32,
                ),
            );
            self.shaders.navigation(
                navigation.key.owner,
                &frame,
                navigation.duration,
                navigation.backwards,
                navigation.reverse_inputs,
            )?;
            changed = true;
        }
        if !changed && !self.shaders.wants_frame() {
            self.shared.animating.store(false, Ordering::Release);
            return Ok(false);
        }
        let x = -(self.settings.insets[0] as f32);
        let y = -(self.settings.insets[1] as f32);
        if direct {
            let Some(source) = self.reader.texture() else {
                return Ok(false);
            };
            self.scene_source = None;
            let (source, dx, dy) =
                self.filters
                    .render(source, &self.settings.filters, self.settings.scale)?;
            let repaint = self.shaders.render(source, changed, self.settings.scale)?;
            self.publish_gpu(repaint, x + dx, y + dy)?;
            if repaint
                && self.window.is_some()
                && let Some(navigation) = &mut self.navigation
                && navigation.snapshot.is_none()
                && !self.shaders.navigation_running(navigation.key.owner)
            {
                navigation.complete = true;
                self.completed = Some(navigation.key);
                self.shared.animating.store(false, Ordering::Release);
            }
            return Ok(repaint);
        }
        self.shared.animating.store(false, Ordering::Release);
        Ok(changed)
    }
    fn publish_gpu(&mut self, repaint: bool, x: f32, y: f32) -> std::result::Result<(), String> {
        self.shared
            .animating
            .store(self.shaders.wants_frame(), Ordering::Release);
        let Some(output) = self.shaders.output() else {
            return Ok(());
        };
        let Some(window) = &mut self.window else {
            return Ok(());
        };
        if !repaint {
            return Ok(());
        }
        output.present(window)?;
        let (_, width, height) = output.picture();
        let frame = RgbaFrame {
            width,
            height,
            pixels: Vec::new(),
        };
        let mut state = self.shared.state.lock().unwrap();
        state.output = Some(Output { frame, x, y });
        Ok(())
    }
}

pub(crate) struct Composite {
    owner: std::rc::Weak<Node>,
    presentation: RefCell<Option<super::presentation::Presentation>>,
    surface: RefCell<Option<String>>,
    pub(crate) generation: u32,
    key: Key,
    shared: Arc<Shared>,
    worker: Rc<Worker>,
}
impl Composite {
    pub(crate) fn presented_node(&self) -> Option<crate::render::node::NativeNode> {
        self.presentation
            .borrow()
            .as_ref()
            .map(|layer| layer.native())
    }

    pub(crate) fn key(&self) -> Key {
        self.key
    }
    pub(crate) fn new(owner: &Rc<Node>, generation: u32) -> Result<Self> {
        Self::create(owner, generation, false)
    }
    pub(crate) fn overlay(owner: &Rc<Node>, generation: u32) -> Result<Self> {
        Self::create(owner, generation, true)
    }
    fn create(owner: &Rc<Node>, generation: u32, overlay: bool) -> Result<Self> {
        let worker = Worker::for_owner(owner)?;
        let shared = Arc::new(Shared {
            released: AtomicBool::new(false),
            active: AtomicBool::new(true),
            state: Mutex::new(State {
                settings: settings(owner, overlay)?,
                screen_origin: None,
                output: None,
                surface: None,
                closed: false,
            }),
            queued: AtomicBool::new(false),
            posted: AtomicBool::new(false),
            animating: AtomicBool::new(false),
        });
        let key = Key {
            id: owner.id,
            generation,
            overlay,
        };
        worker
            .send
            .send(Task::Create {
                key,
                shared: shared.clone(),
            })
            .map_err(|_| napi_ohos::Error::from_reason("OHOS compositor stopped"))?;
        // GPU/EGL initialization completes on the compositor lane; its ready
        // event installs the texture producer in a later UI turn.
        if !overlay {
            owner
                .composite_nodes
                .borrow_mut()
                .push(Rc::downgrade(owner));
        }
        let presentation = if shared.state.lock().unwrap().settings.gpu {
            Some(super::presentation::Presentation::new(
                owner,
                key,
                WindowEvents {
                    target: WindowTarget::Composite(key),
                    send: worker.send.clone(),
                },
            )?)
        } else {
            None
        };
        Ok(Self {
            owner: Rc::downgrade(owner),
            presentation: RefCell::new(presentation),
            surface: RefCell::default(),
            generation,
            key,
            shared,
            worker,
        })
    }
    pub(crate) fn update(&self, owner: &Node) -> Result<()> {
        let settings = settings(owner, self.key.overlay)?;
        let mut state = self.shared.state.lock().unwrap();
        if state.settings != settings {
            state.settings = settings;
            drop(state);
            wake(&self.worker.send, self.key, &self.shared);
            owner
                .visual_node()
                .borrow()
                .mark_dirty(ohos_arkui_binding::types::advanced::NodeDirtyFlag::NeedMeasure)?;
        }
        Ok(())
    }
    pub(crate) fn shader_slot(&self, id: NodeId, slot: u16, value: AnimationValue) {
        let _ = self.worker.send.send(Task::Slot(self.key, id, slot, value));
    }
    pub(crate) fn shader_transition(&self, id: NodeId, duration: std::time::Duration) {
        let _ = self
            .worker
            .send
            .send(Task::Transition(self.key, id, duration));
    }
    pub(crate) fn shader_navigation(
        &self,
        snapshot: super::snapshot::Snapshot,
        key: crate::render::shader::navigation::Key,
        duration: std::time::Duration,
        backwards: bool,
        reverse_inputs: bool,
    ) {
        let _ = self.worker.send.send(Task::Navigation(Navigation {
            target: self.key,
            key,
            snapshot: Some(snapshot),
            duration,
            backwards,
            reverse_inputs,
            complete: false,
        }));
    }
    pub(crate) fn shader_frame(&self, active: bool) -> bool {
        let was_active = self.shared.active.swap(active, Ordering::AcqRel);
        let due = active && self.needs_shader_frame();
        if due && !was_active {
            wake(&self.worker.send, self.key, &self.shared);
        }
        due
    }
    pub(crate) fn needs_shader_frame(&self) -> bool {
        self.shared.animating.load(Ordering::Acquire)
    }
    pub(crate) fn insets(&self) -> crate::interop::sdk::TextureInsets {
        let state = self.shared.state.lock().unwrap();
        let [left, top, right, bottom] = state
            .settings
            .insets
            .map(|v| v as f64 / state.settings.scale as f64);
        crate::interop::sdk::TextureInsets {
            left,
            top,
            right,
            bottom,
        }
    }
    pub(crate) fn origin(&self) -> (f32, f32) {
        let insets = self.insets();
        (-(insets.left as f32), -(insets.top as f32))
    }
    pub(crate) fn surface(&self) -> Option<String> {
        self.surface.borrow().clone()
    }
    pub(crate) fn ready(&self) -> bool {
        let surface = self.shared.state.lock().unwrap().surface.take();
        let initialized = surface.is_some();
        if let Some(result) = surface {
            match result {
                Ok(id) => *self.surface.borrow_mut() = Some(id.to_string()),
                Err(error) => log::warn!("OHOS compositor initialization: {error}"),
            }
        }
        self.shared.posted.store(false, Ordering::Release);
        let output = self.shared.state.lock().unwrap().output.take();
        if let Some(output) = output {
            let mut presentation = self.presentation.borrow_mut();
            if let Some(layer) = presentation.as_mut() {
                let result =
                    layer.update(output.frame.width, output.frame.height, output.x, output.y);
                match result {
                    Ok(()) => {
                        drop(presentation);
                        return initialized;
                    }
                    Err(error) => log::warn!("shader frame presentation: {error}"),
                }
                return initialized;
            }
        }
        initialized
    }
    pub(crate) fn measure(
        &self,
        event: &ohos_arkui_binding::api::node_custom_event::NodeCustomEvent,
    ) -> Result<()> {
        if let Some(layer) = self.presentation.borrow().as_ref() {
            layer.measure(event)?;
        }
        Ok(())
    }
    pub(crate) fn layout(
        &self,
        event: &ohos_arkui_binding::api::node_custom_event::NodeCustomEvent,
    ) -> Result<()> {
        if let Some(owner) = self.owner.upgrade() {
            let origin = owner.mounted().borrow().layout_position_in_screen()?;
            let mut state = self.shared.state.lock().unwrap();
            let origin = Some((origin.x, origin.y));
            if state.screen_origin != origin {
                state.screen_origin = origin;
                drop(state);
                wake(&self.worker.send, self.key, &self.shared);
            }
        }
        if let Some(layer) = self.presentation.borrow().as_ref() {
            layer.layout(event)?;
        }
        Ok(())
    }
}
impl Drop for Composite {
    fn drop(&mut self) {
        if let Some(layer) = self.presentation.get_mut().take() {
            self.worker
                .retired
                .borrow_mut()
                .push((self.shared.clone(), layer));
        }
        self.shared.state.lock().unwrap().closed = true;
        wake(&self.worker.send, self.key, &self.shared);
    }
}
fn settings(owner: &Node, overlay: bool) -> Result<Settings> {
    let frame = owner.frame.get();
    let scene = owner.scene_active();
    let bounds = if overlay {
        crate::render::composition::geometry::Bounds::new(frame.width, frame.height)
    } else if scene {
        crate::render::composition::geometry::Bounds::new(frame.width, frame.height)
            .own(&owner.style.borrow())
    } else {
        owner.capture_bounds(0)?
    };
    let insets = bounds.insets(frame.width, frame.height, owner.scale);
    let width = (frame.width * owner.scale).ceil().max(1.0) as u32;
    let height = (frame.height * owner.scale).ceil().max(1.0) as u32;
    let width = width
        .checked_add(insets[0])
        .and_then(|v| v.checked_add(insets[2]));
    let height = height
        .checked_add(insets[1])
        .and_then(|v| v.checked_add(insets[3]));
    let (Some(width), Some(height)) = (width, height) else {
        return Err(crate::render::node::Error::Unsupported(
            "compositor capture size overflow",
        ));
    };
    if u64::from(width) * u64::from(height) > 16_777_216 {
        return Err(crate::render::node::Error::Unsupported(
            "compositor capture exceeds 16 megapixels",
        ));
    }
    Ok(Settings {
        gpu: !scene && !overlay,
        width,
        height,
        insets,
        scene,
        shaders: if overlay {
            Vec::new()
        } else {
            owner.shader_layers.borrow().clone()
        },
        filters: if overlay {
            Vec::new()
        } else {
            owner
                .style
                .borrow()
                .effects
                .as_ref()
                .map_or_else(Vec::new, |effects| effects.filters.clone())
        },
        scale: owner.scale,
    })
}
