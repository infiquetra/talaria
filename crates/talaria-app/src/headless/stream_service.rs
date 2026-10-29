//! Streaming session service.

use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::Instant;

use hermes_client::liveness::LinkEvent;
use tokio::sync::mpsc::Receiver;

use crate::headless::PrintOptions;
use crate::headless::output::{render_json, render_stderr};
use crate::headless::run::{
    CallReply, HeadlessEffect, HeadlessInput, HeadlessRun, StopCause, SubmitReply,
    stopped_before_submit,
};
use crate::headless::service::{
    Engine, HeadlessTimings, JobReply, open_engine, timed, wait_deadline,
};
use crate::headless::stream::{InputFormat, StreamProjector, StreamRecord, encode};
use crate::headless::stream_input::{InputError, InputItem, InputReader, InputRecord};
use crate::launch::Launch;

pub enum StreamSource {
    Prompt(String),
    Lines(InputReader),
}

/// Test instrumentation called as each owned event becomes eligible for projection.
pub type ObservedHook = Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>;

#[allow(clippy::too_many_arguments)]
pub async fn run_stream<W: Write + Send>(
    launch: &Launch,
    options: &PrintOptions,
    source: StreamSource,
    home: &Path,
    started: Instant,
    stops: Receiver<StopCause>,
    timings: HeadlessTimings,
    out: W,
) -> u8 {
    run(
        launch, options, source, home, started, stops, timings, out, None,
    )
    .await
}

/// Run with an observer of events accepted by the headless state machine.
#[allow(clippy::too_many_arguments)]
pub async fn run_stream_with_observer<W: Write + Send>(
    launch: &Launch,
    options: &PrintOptions,
    source: StreamSource,
    home: &Path,
    started: Instant,
    stops: Receiver<StopCause>,
    timings: HeadlessTimings,
    out: W,
    observer: ObservedHook,
) -> u8 {
    run(
        launch,
        options,
        source,
        home,
        started,
        stops,
        timings,
        out,
        Some(observer),
    )
    .await
}

struct Writer<W: Write> {
    out: Option<BufWriter<W>>,
    closed: bool,
    started: Instant,
}

impl<W: Write> Writer<W> {
    fn elapsed(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
    fn line(&mut self, line: &str) -> bool {
        if self.closed {
            return false;
        }
        let out = self.out.as_mut().expect("writer is present");
        if out
            .write_all(line.as_bytes())
            .and_then(|()| out.flush())
            .is_err()
        {
            self.closed = true;
            return false;
        }
        true
    }
    fn record(&mut self, record: &StreamRecord) -> bool {
        if let StreamRecord::Warning { text, .. } = record {
            let _ = writeln!(std::io::stderr().lock(), "talaria: warning: {text}");
        }
        self.line(&format!("{}\n", encode(record)))
    }
    fn result(&mut self, result: &crate::headless::output::HeadlessResult) -> bool {
        let written = self.line(&render_json(result));
        let _ = std::io::stderr().lock().write_all(&render_stderr(result));
        written
    }
    fn input_error(&mut self, turn: Option<u32>, line: Option<u64>, error: InputError) -> bool {
        let what = error.what(line);
        let _ = writeln!(
            std::io::stderr().lock(),
            "talaria: {what}\nNext step: fix the input line; see docs/formats/stream-v1.md."
        );
        self.record(&StreamRecord::InputError {
            turn,
            line,
            what,
            elapsed_ms: self.elapsed(),
        })
    }
}

impl<W: Write> Drop for Writer<W> {
    fn drop(&mut self) {
        if self.closed
            && let Some(out) = self.out.take()
        {
            let _ = out.into_parts();
        }
    }
}

fn stop_code(cause: StopCause) -> u8 {
    match cause {
        StopCause::Timeout => 124,
        StopCause::Signal(signal) => (128 + signal) as u8,
        StopCause::Record => 130,
        StopCause::OutputClosed => 141,
    }
}

fn stop_for_closed(engine: &mut Engine, stop: &mut Option<StopCause>, active: bool) {
    if active && stop.is_none() {
        *stop = Some(StopCause::OutputClosed);
        engine.feed(HeadlessInput::Stop(StopCause::OutputClosed));
    }
}

#[allow(clippy::too_many_arguments)]
async fn run<W: Write + Send>(
    launch: &Launch,
    _options: &PrintOptions,
    source: StreamSource,
    home: &Path,
    started: Instant,
    mut stops: Receiver<StopCause>,
    timings: HeadlessTimings,
    out: W,
    observer: Option<ObservedHook>,
) -> u8 {
    let mut writer = Writer {
        out: Some(BufWriter::new(out)),
        closed: false,
        started,
    };
    let (format, mut reader, mut pending, mut input_open) = match source {
        StreamSource::Prompt(text) => (
            InputFormat::Text,
            None,
            Some((0, InputItem::Record(InputRecord::User { text }))),
            false,
        ),
        StreamSource::Lines(reader) => (InputFormat::StreamJson, Some(reader.items), None, true),
    };
    let (mut engine, mut stops_open) =
        match open_engine(launch, None, home, &mut stops, timings).await {
            Ok(opened) => opened,
            Err(result) => {
                let result = timed(*result, started);
                writer.result(&result);
                return result.exit_code;
            }
        };
    let mut can_detach = true;
    let mut active = false;
    let mut turn = 0u32;
    let mut projector = StreamProjector::new(0);
    let mut gap = false;
    let mut stored_after_gap = None;
    let mut stop = None;
    let mut interrupt_warned = false;
    let mut input_error: Option<(u64, InputError)> = None;
    let mut last_code = 0;

    // A failed snapshot is delivered through the existing machine's effects.
    if !engine.effects.is_empty() {
        active = true;
        can_detach = false;
    } else if let HeadlessInput::Opened {
        stored_session_id,
        profile,
        model,
        provider,
        ..
    } = &engine.live_opened
    {
        let init = StreamRecord::Init {
            session_id: stored_session_id.clone(),
            profile: profile.clone(),
            model: model.clone(),
            provider: provider.clone(),
            input_format: format,
            elapsed_ms: writer.elapsed(),
        };
        writer.record(&init);
    }

    let code = 'running: loop {
        while let Some(effect) = engine.effects.pop_front() {
            let exit_now = matches!(&effect, HeadlessEffect::ExitNow(_));
            match effect {
                HeadlessEffect::Observed {
                    event_type,
                    payload,
                } => {
                    if let Some(observer) = &observer {
                        observer(&event_type, &payload);
                    }
                    for record in projector.observe(&event_type, &payload, writer.elapsed()) {
                        if !writer.record(&record) {
                            stop_for_closed(&mut engine, &mut stop, active);
                        }
                    }
                }
                HeadlessEffect::Recover => {
                    if active && !gap {
                        gap = true;
                        let record = projector.mark_gap(writer.elapsed());
                        if !writer.record(&record) {
                            stop_for_closed(&mut engine, &mut stop, active);
                        }
                    }
                    engine.execute(HeadlessEffect::Recover);
                }
                HeadlessEffect::Detach => can_detach = true,
                HeadlessEffect::Finish(result) | HeadlessEffect::ExitNow(result) => {
                    let mut result = timed(result, started);
                    if let Some(text) = stored_after_gap.take() {
                        result.text = text;
                        let warning = "The answer was read from Hermes's stored copy, in which secrets are redacted.";
                        if !result.warnings.iter().any(|existing| existing == warning) {
                            result.warnings.push(warning.into());
                        }
                    }
                    for warning in &result.warnings {
                        // The result renderer also sends these warnings to standard error.
                        let record = StreamRecord::Warning {
                            turn: (turn > 0).then_some(turn),
                            text: warning.clone(),
                            elapsed_ms: writer.elapsed(),
                        };
                        if !writer.closed {
                            writer.line(&format!("{}\n", encode(&record)));
                        }
                    }
                    writer.result(&result);
                    last_code = result.exit_code;
                    active = false;
                    if exit_now {
                        can_detach = false;
                        break 'running last_code;
                    }
                    if let Some(cause @ (StopCause::Signal(_) | StopCause::Timeout)) = stop {
                        break 'running stop_code(cause);
                    }
                    if writer.closed {
                        break 'running 141;
                    }
                    if let Some((line, error)) = input_error.take() {
                        if !writer.input_error((turn > 0).then_some(turn), Some(line), error) {
                            break 'running 141;
                        }
                        break 'running 2;
                    }
                    if last_code != 0 && !(last_code == 130 && stop == Some(StopCause::Record)) {
                        break 'running last_code;
                    }
                    stop = None;
                    engine.budget = None;
                    engine.settle = None;
                }
                other => engine.execute(other),
            }
        }
        if writer.closed && !active {
            break 141;
        }

        // Give stops and already-received session changes priority before another turn.
        if !active {
            if stops_open {
                match stops.try_recv() {
                    Ok(cause) => {
                        let result = stopped_before_submit(cause);
                        let _ = std::io::stderr().lock().write_all(&render_stderr(&result));
                        break 'running stop_code(cause);
                    }
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => stops_open = false,
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
                }
            }
            while let Ok(event) = engine.inbound.try_recv() {
                engine.link_event(event);
            }
        }

        // A buffered user stays at the head until the preceding result is written.
        if !active && let Some((line, item)) = pending.take() {
            match item {
                InputItem::Record(InputRecord::User { text }) => {
                    turn = turn.saturating_add(1);
                    engine.turn = u64::from(turn);
                    engine.machine = HeadlessRun::new(text);
                    active = true;
                    can_detach = false;
                    projector = StreamProjector::new(turn);
                    gap = false;
                    stored_after_gap = None;
                    interrupt_warned = false;
                    // Take already-queued controls before opening the turn's machine.
                    if let Some(reader) = &mut reader {
                        loop {
                            match reader.try_recv() {
                                Ok((_, InputItem::Record(InputRecord::Interrupt))) => {
                                    if stop.is_none() {
                                        stop = Some(StopCause::Record);
                                        engine.feed(HeadlessInput::Stop(StopCause::Record));
                                    } else if !interrupt_warned {
                                        interrupt_warned = true;
                                        let warning = StreamRecord::Warning {
                                            turn: Some(turn),
                                            text:
                                                "interrupt ignored: the turn is already stopping."
                                                    .into(),
                                            elapsed_ms: writer.elapsed(),
                                        };
                                        if !writer.record(&warning) {
                                            stop_for_closed(&mut engine, &mut stop, active);
                                        }
                                    }
                                }
                                Ok((line, InputItem::Error(error))) => {
                                    input_error = Some((line, error));
                                    if stop.is_none() {
                                        stop = Some(StopCause::Record);
                                        engine.feed(HeadlessInput::Stop(StopCause::Record));
                                    }
                                    break;
                                }
                                Ok(item) => {
                                    pending = Some(item);
                                    break;
                                }
                                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                                    input_open = false;
                                    break;
                                }
                                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                            }
                        }
                    }
                    engine.feed(engine.live_opened.clone());
                    continue;
                }
                InputItem::Record(InputRecord::Interrupt) => {
                    let record = StreamRecord::Warning {
                        turn: (turn > 0).then_some(turn),
                        text: "nothing to interrupt.".into(),
                        elapsed_ms: writer.elapsed(),
                    };
                    if !writer.record(&record) {
                        break 141;
                    }
                    continue;
                }
                InputItem::Error(error) => {
                    if !writer.input_error((turn > 0).then_some(turn), Some(line), error) {
                        break 141;
                    }
                    break 2;
                }
            }
        }
        if !active && !input_open && pending.is_none() {
            if turn == 0 {
                if !writer.input_error(None, None, InputError::NoUserRecord) {
                    break 141;
                }
                break 2;
            }
            break last_code;
        }

        tokio::select! {
            biased;
            cause = stops.recv(), if stops_open => match cause {
                Some(cause) if active => { stop = Some(cause); engine.feed(HeadlessInput::Stop(cause)); }
                Some(cause) => {
                    let result = stopped_before_submit(cause);
                    let _ = std::io::stderr().lock().write_all(&render_stderr(&result));
                    break 'running stop_code(cause);
                }
                None => stops_open = false,
            },
            _ = wait_deadline(engine.budget) => {
                engine.budget = None;
                can_detach = false;
                engine.feed(HeadlessInput::StopBudgetExpired);
            },
            _ = wait_deadline(engine.settle) => { engine.settle = None; engine.feed(HeadlessInput::SettleDeadline); },
            item = async { reader.as_mut().expect("open reader").recv().await }, if input_open && pending.is_none() && input_error.is_none() => match item {
                Some((_line, InputItem::Record(InputRecord::Interrupt))) if active => {
                    if stop.is_some() {
                        if !interrupt_warned {
                            interrupt_warned = true;
                            let record = StreamRecord::Warning { turn: Some(turn), text: "interrupt ignored: the turn is already stopping.".into(), elapsed_ms: writer.elapsed() };
                            if !writer.record(&record) { stop_for_closed(&mut engine, &mut stop, active); }
                        }
                    } else { stop = Some(StopCause::Record); engine.feed(HeadlessInput::Stop(StopCause::Record)); }
                }
                Some((line, InputItem::Error(error))) if active => {
                    input_error = Some((line, error));
                    if stop.is_none() { stop = Some(StopCause::Record); engine.feed(HeadlessInput::Stop(StopCause::Record)); }
                }
                Some(item) => pending = Some(item),
                None => input_open = false,
            },
            event = engine.inbound.recv(), if engine.inbound_open => match event {
                Some(event) => {
                    if active && !gap && matches!(&event, LinkEvent::Gap(g) if g.session_id == engine.opened.session_id) {
                        gap = true;
                        let record = projector.mark_gap(writer.elapsed());
                        if !writer.record(&record) { stop_for_closed(&mut engine, &mut stop, active); }
                    }
                    engine.link_event(event);
                }
                None => { engine.inbound_open = false; engine.feed(HeadlessInput::LinkFailed { reason: "the connection closed".into(), next_step: "Check the gateway with talaria, then check the session.".into() }); }
            },
            reply = engine.jobs.join_next(), if !engine.jobs.is_empty() => match reply {
                Some(Ok(reply)) => {
                    if gap && let JobReply::Input(HeadlessInput::ReceiptRead(CallReply::Ok(value))) = &reply
                        && value.get("status").and_then(serde_json::Value::as_str) == Some("terminal")
                        && let Some(text) = value.pointer("/result/final_response").and_then(serde_json::Value::as_str)
                    {
                        stored_after_gap = Some(text.to_owned());
                    }
                    let submitted = match &reply {
                        JobReply::Input(HeadlessInput::SubmitReplied(SubmitReply::Admitted { admission_id, status, .. })) if matches!(status.as_str(), "queued" | "started" | "terminal") => Some(StreamRecord::Submitted { turn, admission_id: admission_id.clone(), status: status.clone(), elapsed_ms: writer.elapsed() }),
                        _ => None,
                    };
                    engine.job(reply);
                    if let Some(record) = submitted && !writer.record(&record) { stop_for_closed(&mut engine, &mut stop, active); }
                }
                Some(Err(_)) => engine.feed(HeadlessInput::LinkFailed { reason: "a gateway call stopped".into(), next_step: "Check the gateway with talaria, then check the session.".into() }),
                None => {},
            },
        }
    };

    if can_detach {
        engine.execute(HeadlessEffect::Detach);
        while engine.detaching {
            match engine.jobs.join_next().await {
                Some(Ok(reply)) => engine.job(reply),
                _ => break,
            }
        }
    }
    engine.close().await;
    code
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    struct Closed(Arc<AtomicUsize>);
    impl Write for Closed {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn closed_writer_does_not_retry_buffer_on_drop() {
        let writes = Arc::new(AtomicUsize::new(0));
        {
            let mut writer = Writer {
                out: Some(BufWriter::new(Closed(writes.clone()))),
                closed: false,
                started: Instant::now(),
            };
            assert!(!writer.line("first\n"));
            assert!(!writer.line("second\n"));
        }
        assert_eq!(writes.load(Ordering::SeqCst), 1);
    }
}
