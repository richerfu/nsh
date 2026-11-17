//! An external display-frame request completes after its turn commits and
//! requests any following frame. Automatic frames have no external receipt.

use std::time::{Duration, SystemTime};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::{
    ApproxTextEngine, MountBatch, MountItem,
    element::Root,
    prelude::*,
    time::{Clock, Instant},
};
use nsh_engine::{App, Bundle, Engine, EngineConfig, EngineControl, LocalEngine, MountSink};

#[derive(Debug)]
enum Sent {
    Batch(MountBatch),
    Requested,
    Completed(u32, Instant),
    Error(String),
    Advanced,
}

struct Sink(Sender<Sent>);

impl MountSink for Sink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(Sent::Batch(batch));
    }

    fn request_frame(&self) {
        let _ = self.0.send(Sent::Requested);
    }

    fn report_error(&self, message: &str) {
        let _ = self.0.send(Sent::Error(message.to_owned()));
    }
}

const RAF: &str = r#"
let frames = 0;
function frame() {
  if (++frames < 2) requestAnimationFrame(frame);
}
requestAnimationFrame(frame);
"#;

fn config(source: &str, clock: Clock) -> EngineConfig {
    EngineConfig {
        clock,
        ..EngineConfig::new(App::Js(Bundle::source(source.to_owned())))
    }
}

fn start(
    source: &str,
    clock: Clock,
) -> (LocalEngine<ApproxTextEngine>, Receiver<Sent>, Sender<Sent>) {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut engine =
        LocalEngine::start(config(source, clock), ApproxTextEngine, Sink(tx.clone())).unwrap();
    engine.pump();
    let initial: Vec<_> = rx.try_iter().collect();
    assert!(initial.iter().any(|item| matches!(item, Sent::Requested)));
    assert!(
        initial
            .iter()
            .all(|item| matches!(item, Sent::Batch(_) | Sent::Requested))
    );
    (engine, rx, tx)
}

fn frame(control: &EngineControl, tx: &Sender<Sent>, id: u32) {
    let tx = tx.clone();
    control.frame_with_completion(move || {
        let _ = tx.send(Sent::Completed(id, nsh_core::time::now()));
    });
}

#[test]
fn frames_complete_without_batches_and_request_the_next_frame_first() {
    for clock in [
        Clock::system(),
        Clock::manual(SystemTime::UNIX_EPOCH),
        Clock::manual(SystemTime::UNIX_EPOCH).with_frames(),
    ] {
        let (mut engine, rx, tx) = start(RAF, clock);
        frame(&engine.control(), &tx, 1);
        engine.pump();
        let first: Vec<_> = rx.try_iter().collect();
        assert!(
            matches!(first.as_slice(), [Sent::Requested, Sent::Completed(1, _)]),
            "{first:?}"
        );

        frame(&engine.control(), &tx, 2);
        engine.pump();
        let last: Vec<_> = rx.try_iter().collect();
        assert!(
            matches!(last.as_slice(), [Sent::Completed(2, _)]),
            "{last:?}"
        );

        // An ordinary engine turn does not complete another display frame.
        engine.pump();
        assert!(rx.try_recv().is_err());
    }
}

#[test]
fn frames_complete_on_the_engine_thread_without_a_mount_batch() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let engine = Engine::start(
        config(RAF, Clock::system()),
        || ApproxTextEngine,
        Sink(tx.clone()),
    )
    .unwrap();
    loop {
        match rx.recv_timeout(Duration::from_secs(5)).unwrap() {
            Sent::Batch(_) => {}
            Sent::Requested => break,
            other => panic!("unexpected startup report: {other:?}"),
        }
    }
    frame(&engine.control(), &tx, 1);
    assert!(matches!(
        rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        Sent::Requested
    ));
    assert!(matches!(
        rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        Sent::Completed(1, _)
    ));
    frame(&engine.control(), &tx, 2);
    assert!(matches!(
        rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        Sent::Completed(2, _)
    ));
    // Join the real engine, rather than guessing that it finished after a delay.
    drop(engine);
    assert!(rx.try_recv().is_err());
}

#[component]
fn FrameText() -> impl IntoElement {
    let value = signal("before");
    spawn(async move {
        next_frame().await;
        value.set("after");
        next_frame().await;
    });
    text(move || value.get())
}

#[test]
fn frames_complete_after_their_commit_and_rust_tasks_request_again() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        EngineConfig::new(App::Rust(Root::of::<FrameText>())),
        ApproxTextEngine,
        Sink(tx.clone()),
    )
    .unwrap();
    engine.pump();
    assert!(rx.try_iter().any(|item| matches!(item, Sent::Requested)));
    frame(&engine.control(), &tx, 1);
    engine.pump();
    let reports: Vec<_> = rx.try_iter().collect();
    let [Sent::Batch(batch), Sent::Requested, Sent::Completed(1, _)] = reports.as_slice() else {
        panic!("unexpected frame reports: {reports:?}");
    };
    assert!(
        batch
            .items
            .iter()
            .any(|item| matches!(item, MountItem::Text { text, .. } if text.text == "after"))
    );
}

#[test]
fn frames_complete_only_after_earlier_timers_and_nested_clock_messages() {
    let clock = Clock::manual(SystemTime::UNIX_EPOCH).with_frames();
    let source = r#"
let frames = 0;
function frame() {
  if (++frames === 1) {
    setTimeout(() => { throw new Error('earlier timer'); }, 1);
    requestAnimationFrame(frame);
  }
}
requestAnimationFrame(frame);
"#;
    let (mut engine, rx, tx) = start(source, clock.clone());
    // The original API still runs the callbacks and asks for their next
    // frame, without acquiring a completion callback of its own.
    engine.control().frame();
    engine.pump();
    let first: Vec<_> = rx.try_iter().collect();
    assert!(matches!(first.as_slice(), [Sent::Requested]));
    let start_time = clock.now();
    let control = engine.control();
    let (advance_tx, advanced) = crossbeam_channel::unbounded();
    // A clock alarm queues a nested move before the display frame's time.
    // This is input to the test, not a completion fence: it must not finish
    // the outer Frame message before that frame's callbacks have run.
    clock.at(start_time + Duration::from_millis(2), move || {
        control.advance_clock(Duration::ZERO, move || {
            let _ = advance_tx.send(nsh_core::time::now());
        });
    });
    frame(&engine.control(), &tx, 2);
    engine.pump();
    assert_eq!(
        advanced.try_recv().unwrap() - start_time,
        Duration::from_millis(2)
    );
    let reports: Vec<_> = rx.try_iter().collect();
    let [Sent::Error(message), Sent::Completed(2, at)] = reports.as_slice() else {
        panic!("unexpected nested frame reports: {reports:?}");
    };
    assert!(message.contains("earlier timer"));
    assert_eq!(*at - start_time, nsh_core::time::FRAME);
}

#[test]
fn frames_run_by_a_manual_clock_do_not_complete_external_requests() {
    let clock = Clock::manual(SystemTime::UNIX_EPOCH).with_frames();
    let start_time = clock.now();
    let (mut engine, rx, tx) = start(RAF, clock.clone());
    let advance_tx = tx.clone();
    engine
        .control()
        .advance_clock(nsh_core::time::FRAME * 3, move || {
            let _ = advance_tx.send(Sent::Advanced);
        });
    engine.pump();
    let reports: Vec<_> = rx.try_iter().collect();
    assert!(
        matches!(reports.as_slice(), [Sent::Requested, Sent::Advanced]),
        "{reports:?}"
    );
    assert_eq!(clock.now() - start_time, nsh_core::time::FRAME * 3);

    frame(&engine.control(), &tx, 3);
    engine.pump();
    let reports: Vec<_> = rx.try_iter().collect();
    assert!(
        matches!(reports.as_slice(), [Sent::Completed(3, _)]),
        "{reports:?}"
    );
}

#[test]
fn frames_nested_by_a_manual_clock_keep_their_own_completions() {
    let clock = Clock::manual(SystemTime::UNIX_EPOCH).with_frames();
    let (mut engine, rx, tx) = start(RAF, clock.clone());
    engine.control().frame();
    engine.pump();
    assert!(matches!(
        rx.try_iter().collect::<Vec<_>>().as_slice(),
        [Sent::Requested]
    ));
    let start_time = clock.now();
    let control = engine.control();
    let nested_tx = tx.clone();
    clock.at(start_time + Duration::from_millis(2), move || {
        frame(&control, &nested_tx, 3);
    });
    frame(&engine.control(), &tx, 2);
    engine.pump();
    let reports: Vec<_> = rx.try_iter().collect();
    // The nested request returns first. Both receipts belong to their
    // requests even though the clock already delivered the outer frame.
    let [Sent::Completed(3, nested), Sent::Completed(2, outer)] = reports.as_slice() else {
        panic!("unexpected nested frame completions: {reports:?}");
    };
    assert_eq!(*nested - start_time, nsh_core::time::FRAME);
    assert_eq!(*outer, *nested);
}

#[test]
fn frames_coalesced_into_one_turn_each_complete() {
    let (mut engine, rx, tx) = start(RAF, Clock::system());
    frame(&engine.control(), &tx, 1);
    frame(&engine.control(), &tx, 2);
    engine.pump();
    let reports: Vec<_> = rx.try_iter().collect();
    assert!(
        matches!(
            reports.as_slice(),
            [Sent::Completed(1, _), Sent::Completed(2, _)]
        ),
        "{reports:?}"
    );
}

#[test]
fn frames_with_a_callback_error_still_complete() {
    let (mut engine, rx, tx) = start(
        "requestAnimationFrame(() => { throw new Error('frame failed'); });",
        Clock::system(),
    );
    frame(&engine.control(), &tx, 1);
    engine.pump();
    let reports: Vec<_> = rx.try_iter().collect();
    assert!(
        matches!(reports.as_slice(), [Sent::Error(message), Sent::Completed(1, _)] if message.contains("frame failed")),
        "{reports:?}"
    );
}

#[test]
fn frames_requested_before_start_complete_after_the_first_commit() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start_with(
        config(RAF, Clock::system()),
        ApproxTextEngine,
        Sink(tx.clone()),
        |control| frame(control, &tx, 1),
    )
    .unwrap();
    let reports: Vec<_> = rx.try_iter().collect();
    assert!(
        matches!(reports.last(), Some(Sent::Completed(1, _))),
        "{reports:?}"
    );
    assert!(
        reports[..reports.len() - 1]
            .iter()
            .any(|report| matches!(report, Sent::Requested))
    );
    assert!(
        reports[..reports.len() - 1]
            .iter()
            .all(|report| matches!(report, Sent::Batch(_) | Sent::Requested))
    );

    engine.control().frame();
    engine.pump();
    assert!(rx.try_recv().is_err());
}
