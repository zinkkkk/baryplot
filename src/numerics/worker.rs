//! Disposable numerical processes. Kill is independent of faer's cancellation support.
use super::{AnalysisOptions, Spectrum, aaa};
use crate::{
    expression::Variables,
    model::{Compiled, Node, Project},
};
use num_complex::Complex64 as C;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    io::{self, BufReader, Read, Write},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

const MAGIC: &[u8; 8] = b"BARYIPC1";
const MAX_FRAME: usize = 256 * 1024 * 1024;
#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    Fit(Box<Project>),
    Spectrum {
        nodes: Vec<Node>,
        options: AnalysisOptions,
        reference: Option<Arc<[[C; 2]]>>,
    },
    #[cfg(test)]
    Block,
    #[cfg(test)]
    Crash,
}
#[derive(Debug, Serialize, Deserialize)]
pub enum Event {
    Progress(aaa::Progress),
    Fitted(Result<Box<aaa::Fit>, String>),
    Analyzed(Result<Box<Spectrum>, String>),
    Failed(String),
    Cancelled,
    #[cfg(test)]
    Ready,
}
impl Event {
    fn terminal(&self) -> bool {
        match self {
            Self::Progress(_) => false,
            #[cfg(test)]
            Self::Ready => false,
            _ => true,
        }
    }
}
#[derive(Default)]
struct Control {
    child: Mutex<Option<Child>>,
    wake: Condvar,
    cancelled: AtomicBool,
    finished: AtomicBool,
}
impl Control {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        // This mutex only protects handle registration / kill, never IO or wait.
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(child) = child.as_mut() {
            let _ = child.kill();
        }
        self.wake.notify_all();
    }
    fn finish(&self) {
        let child = self.child.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(mut child) = child {
            // Also dispose of a worker stuck during shutdown after sending a result.
            let _ = child.kill();
            let _ = child.wait(); // Reap only on the manager thread.
        }
        self.finished.store(true, Ordering::Release);
    }
}
/// Owned by the GUI. Dropping it kills the calculation, including all Rayon threads.
#[derive(Debug)]
pub struct Job(Arc<Control>);
impl std::fmt::Debug for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NumericalProcess")
            .field("cancelled", &self.cancelled.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
struct Reaper(Arc<Control>);
impl Drop for Reaper {
    fn drop(&mut self) {
        self.0.finish();
    }
}
impl Job {
    pub fn spawn(
        request: Request,
        delay: Duration,
    ) -> Result<(Self, smol::channel::Receiver<Event>), String> {
        let control = Arc::new(Control::default());
        let manager = Arc::clone(&control);
        let (sender, receiver) = smol::channel::unbounded();
        std::thread::Builder::new()
            .name("baryplot-process-manager".into())
            .spawn(move || {
                let reaper = Reaper(Arc::clone(&manager));
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    manage(&manager, request, delay, &sender)
                }))
                .unwrap_or_else(|_| Err("Numerical process manager failed".into()));
                drop(reaper);
                let event = if manager.cancelled.load(Ordering::Acquire) {
                    Event::Cancelled
                } else {
                    result.unwrap_or_else(Event::Failed)
                };
                let _ = sender.try_send(event);
            })
            .map_err(|e| format!("Could not start numerical process manager: {e}"))?;
        Ok((Self(control), receiver))
    }
}
fn command() -> io::Result<Command> {
    let mut command = Command::new(std::env::current_exe()?);
    #[cfg(not(test))]
    command.arg("--numerical-worker");
    #[cfg(test)]
    command
        .args([
            "--exact",
            "numerics::worker::tests::worker_entry",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("BARYPLOT_TEST_WORKER", "1");
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    Ok(command)
}
fn manage(
    control: &Control,
    request: Request,
    delay: Duration,
    sender: &smol::channel::Sender<Event>,
) -> Result<Event, String> {
    if !delay.is_zero() {
        let guard = control.child.lock().unwrap_or_else(|e| e.into_inner());
        let _ = control
            .wake
            .wait_timeout_while(guard, delay, |_| !control.cancelled.load(Ordering::Acquire))
            .unwrap_or_else(|e| e.into_inner());
    }
    if control.cancelled.load(Ordering::Acquire) {
        return Ok(Event::Cancelled);
    }
    let mut child = command()
        .and_then(|mut command| command.spawn())
        .map_err(|e| format!("Could not start numerical worker: {e}"))?;
    let mut input = child.stdin.take().expect("piped worker input");
    let output = child.stdout.take().expect("piped worker output");
    {
        let mut slot = control.child.lock().unwrap_or_else(|e| e.into_inner());
        if control.cancelled.load(Ordering::Acquire) {
            let _ = child.kill();
        }
        *slot = Some(child);
    }
    write_frame(&mut input, &request)
        .map_err(|e| format!("Could not send numerical inputs: {e}"))?;
    // Keep input alive as a lease: the worker exits if the parent disappears.
    let mut reader = BufReader::new(output);
    let mut first = true;
    loop {
        let event: Event = read_frame(&mut reader, first)
            .map_err(|e| format!("Numerical worker ended without a result: {e}"))?;
        first = false;
        if event.terminal() {
            return Ok(event);
        }
        if sender.try_send(event).is_err() {
            return Ok(Event::Cancelled);
        }
    }
}
fn write_frame(writer: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    // MessagePack retains infinities/NaNs in diagnostic errors, unlike JSON.
    let bytes = rmp_serde::to_vec_named(value).map_err(io::Error::other)?;
    if bytes.len() > MAX_FRAME {
        return Err(io::Error::other("Numerical message exceeds size limit"));
    }
    writer.write_all(MAGIC)?;
    writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()
}
fn read_frame<T: DeserializeOwned>(reader: &mut impl Read, preamble: bool) -> io::Result<T> {
    let mut magic = [0; 8];
    reader.read_exact(&mut magic)?;
    // The Rust test harness writes a short header before worker_entry. Production
    // workers write no preamble. Scan only before the first frame, with a strict cap.
    if preamble {
        let mut skipped = 0;
        while &magic != MAGIC && skipped < 8192 {
            magic.rotate_left(1);
            reader.read_exact(&mut magic[7..])?;
            skipped += 1;
        }
    }
    if &magic != MAGIC {
        return Err(io::Error::other("Invalid numerical message header"));
    }
    let mut size = [0; 4];
    reader.read_exact(&mut size)?;
    let size = u32::from_le_bytes(size) as usize;
    if size > MAX_FRAME {
        return Err(io::Error::other("Numerical message exceeds size limit"));
    }
    let mut bytes = vec![0; size];
    reader.read_exact(&mut bytes)?;
    rmp_serde::from_slice(&bytes).map_err(io::Error::other)
}
/// Internal entry point in the same executable; it never initializes iced/wgpu.
pub fn serve() -> Result<(), String> {
    let request: Request = read_frame(&mut io::stdin().lock(), false).map_err(|e| e.to_string())?;
    std::thread::Builder::new()
        .name("baryplot-parent-lease".into())
        .spawn(|| {
            let mut byte = [0];
            loop {
                match io::stdin().read(&mut byte) {
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    _ => std::process::exit(0),
                }
            }
        })
        .map_err(|e| e.to_string())?;
    let mut output = io::stdout();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let latest = AtomicU64::new(0);
        match request {
            Request::Fit(project) => {
                let result = aaa::Request::new(&project).and_then(|request| {
                    let mut last_progress = None;
                    let mut phase = None;
                    aaa::fit(&request, 0, &latest, |progress| {
                        let now = std::time::Instant::now();
                        if phase != Some(progress.phase())
                            || last_progress.is_none_or(|last| {
                                now.duration_since(last) >= Duration::from_millis(32)
                            })
                        {
                            phase = Some(progress.phase());
                            last_progress = Some(now);
                            let _ = write_frame(&mut output, &Event::Progress(progress));
                        }
                    })
                    .and_then(|fit| {
                        fit.map(Box::new)
                            .ok_or_else(|| "Fit ended without a result".into())
                    })
                });
                Event::Fitted(result)
            }
            Request::Spectrum {
                nodes,
                options,
                reference,
            } => {
                let result = Compiled::from_nodes(nodes, Variables::new()).and_then(|model| {
                    super::analyze_with_reference(&model, options, 0, &latest, reference.as_deref())
                        .and_then(|spectrum| {
                            spectrum
                                .map(Box::new)
                                .ok_or_else(|| "Analysis ended without a result".into())
                        })
                });
                Event::Analyzed(result)
            }
            #[cfg(test)]
            Request::Block => {
                write_frame(&mut output, &Event::Ready).expect("ready message");
                // Deliberately never checks cancellation, just like a library call.
                loop {
                    std::thread::park();
                }
            }
            #[cfg(test)]
            Request::Crash => std::process::exit(17),
        }
    }))
    .unwrap_or_else(|_| Event::Failed("Numerical worker panicked".into()));
    write_frame(&mut output, &result).map_err(|e| e.to_string())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn receive(receiver: &smol::channel::Receiver<Event>) -> Event {
        smol::block_on(smol::future::or(
            async { receiver.recv().await.expect("worker message") },
            async {
                smol::Timer::after(Duration::from_secs(5)).await;
                panic!("worker timed out");
            },
        ))
    }
    fn project() -> Project {
        let mut project = Project {
            globals: "target(x)=exp(x)".into(),
            ..Project::default()
        };
        project.fit.function = "target".into();
        project
    }
    #[test]
    fn binary_frames_retain_infinite_and_nan_error_estimates() {
        for error in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 1e-250] {
            let event = Event::Progress(aaa::Progress::Iteration(aaa::Step {
                nodes: 4,
                grid: 40,
                error,
                relative_l2: error,
            }));
            let mut bytes = Vec::new();
            write_frame(&mut bytes, &event).expect("write frame");
            let decoded: Event = read_frame(&mut bytes.as_slice(), false).expect("read frame");
            let Event::Progress(aaa::Progress::Iteration(step)) = decoded else {
                panic!("wrong message")
            };
            if error.is_nan() {
                assert!(step.error.is_nan());
            } else {
                assert_eq!(step.error, error);
            }
        }
    }
    #[test]
    fn worker_process_returns_fit_and_spectral_results() {
        for method in [aaa::Method::Classic, aaa::Method::Nonlinear] {
            let mut project = project();
            project.globals[0].kind = crate::model::DefinitionKind::NumericFunction;
            project.globals[0].expression = "let z=x;\nexp(z)".into();
            project.fit.method = method;
            let (job, receiver) =
                Job::spawn(Request::Fit(Box::new(project)), Duration::ZERO).expect("spawn fit");
            let fit = loop {
                match receive(&receiver) {
                    Event::Progress(_) => {}
                    Event::Fitted(result) => break result.expect("fit result"),
                    event => panic!("unexpected {event:?}"),
                }
            };
            assert!(fit.converged);
            assert_eq!(fit.method, method);
            assert!(fit.relative_l2 < 1e-10);
            let model = Compiled::from_nodes(fit.nodes, Variables::new()).expect("fitted model");
            assert!(
                (model.evaluate(C::new(0.37, 0.)).expect("finite") - C::new(0.37_f64.exp(), 0.))
                    .norm()
                    < 1e-12
            );
            assert!(job.0.finished.load(Ordering::Acquire));
        }
        let model = crate::model::reference_project(crate::model::Mode::Real)
            .compile()
            .expect("polynomial");
        let (job, receiver) = Job::spawn(
            Request::Spectrum {
                nodes: model.nodes,
                options: AnalysisOptions {
                    poles: true,
                    zeros: true,
                    residues: true,
                },
                reference: None,
            },
            Duration::ZERO,
        )
        .expect("spawn spectrum");
        let Event::Analyzed(result) = receive(&receiver) else {
            panic!("spectrum result")
        };
        let spectrum = result.expect("analysis");
        assert!(spectrum.poles.is_empty());
        assert_eq!(spectrum.zeros.len(), 2);
        assert!(job.0.finished.load(Ordering::Acquire));
    }
    #[test]
    fn dropping_a_blocked_worker_kills_and_reaps_it_without_cooperation() {
        let (job, receiver) =
            Job::spawn(Request::Block, Duration::ZERO).expect("spawn blocked worker");
        assert!(matches!(receive(&receiver), Event::Ready));
        let control = Arc::clone(&job.0);
        let started = std::time::Instant::now();
        drop(job);
        assert!(
            started.elapsed() < Duration::from_millis(250),
            "cancel waited for numerical code"
        );
        assert!(matches!(receive(&receiver), Event::Cancelled));
        assert!(control.finished.load(Ordering::Acquire));
        assert!(
            control.child.lock().expect("child mutex").is_none(),
            "worker was not reaped"
        );
        let (next, receiver) =
            Job::spawn(Request::Fit(Box::new(project())), Duration::ZERO).expect("replacement");
        loop {
            match receive(&receiver) {
                Event::Progress(_) => {}
                Event::Fitted(result) => {
                    assert!(result.expect("new fit").converged);
                    break;
                }
                event => panic!("unexpected {event:?}"),
            }
        }
        assert!(next.0.finished.load(Ordering::Acquire));
    }
    #[test]
    fn cancellation_before_start_skips_the_decomposition_and_failures_are_reaped() {
        let (job, receiver) =
            Job::spawn(Request::Block, Duration::from_secs(10)).expect("deferred worker");
        let control = Arc::clone(&job.0);
        drop(job);
        assert!(matches!(receive(&receiver), Event::Cancelled));
        assert!(control.finished.load(Ordering::Acquire));
        assert!(control.child.lock().expect("child mutex").is_none());
        let (job, receiver) = Job::spawn(Request::Crash, Duration::ZERO).expect("crashing worker");
        assert!(matches!(receive(&receiver), Event::Failed(_)));
        assert!(job.0.finished.load(Ordering::Acquire));
    }
    #[test]
    fn a_worker_exits_when_its_parent_input_lease_closes() {
        let mut child = command().and_then(|mut c| c.spawn()).expect("spawn worker");
        let mut input = child.stdin.take().expect("input");
        let mut output = child.stdout.take().expect("output");
        let control = Arc::new(Control::default());
        *control.child.lock().expect("child mutex") = Some(child);
        let _cleanup = Reaper(Arc::clone(&control));
        write_frame(&mut input, &Request::Block).expect("request");
        assert!(matches!(
            read_frame::<Event>(&mut output, true).expect("ready"),
            Event::Ready
        ));
        drop(input);
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            let exited = control
                .child
                .lock()
                .expect("child mutex")
                .as_mut()
                .expect("child")
                .try_wait()
                .expect("try wait")
                .is_some();
            if exited {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "orphaned worker did not exit"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    #[ignore = "internal subprocess entry point"]
    fn worker_entry() {
        if std::env::var_os("BARYPLOT_TEST_WORKER").is_some() {
            let result = serve();
            if let Err(e) = &result {
                eprintln!("{e}");
            }
            std::process::exit(if result.is_ok() { 0 } else { 1 });
        }
    }
}
