use std::{
    fmt,
    future::IntoFuture,
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use axum::Router;
use tokio::{
    net::TcpListener,
    sync::oneshot,
    time::{Instant, sleep_until},
};

use crate::{
    config::AppConfig,
    error::{AppError, AppResult},
};

#[derive(Debug, Clone)]
pub struct Lifecycle {
    ready: Arc<AtomicBool>,
}

impl Lifecycle {
    pub fn new() -> Self {
        Self {
            ready: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    pub fn begin_shutdown(&self) {
        self.ready.store(false, Ordering::Release);
    }
}

#[derive(Debug, Clone, Copy)]
struct ShutdownSettings {
    readiness_delay: Duration,
    timeout: Duration,
    timeout_ms: u64,
}

impl From<&AppConfig> for ShutdownSettings {
    fn from(config: &AppConfig) -> Self {
        Self {
            readiness_delay: Duration::from_millis(config.shutdown_readiness_delay_ms),
            timeout: Duration::from_millis(config.shutdown_timeout_ms),
            timeout_ms: config.shutdown_timeout_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShutdownSignal {
    Sigterm,
    CtrlC,
}

impl ShutdownSignal {
    fn as_str(self) -> &'static str {
        match self {
            Self::Sigterm => "SIGTERM",
            Self::CtrlC => "SIGINT",
        }
    }
}

impl fmt::Display for ShutdownSignal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

trait SignalSource {
    async fn receive(&mut self) -> io::Result<ShutdownSignal>;
}

#[cfg(unix)]
struct OsSignalSource {
    terminate: tokio::signal::unix::Signal,
}

#[cfg(unix)]
impl OsSignalSource {
    fn new() -> io::Result<Self> {
        Ok(Self {
            terminate: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?,
        })
    }
}

#[cfg(unix)]
impl SignalSource for OsSignalSource {
    async fn receive(&mut self) -> io::Result<ShutdownSignal> {
        tokio::select! {
            result = tokio::signal::ctrl_c() => result.map(|()| ShutdownSignal::CtrlC),
            signal = self.terminate.recv() => signal
                .map(|()| ShutdownSignal::Sigterm)
                .ok_or_else(|| io::Error::other("SIGTERM signal stream closed")),
        }
    }
}

#[cfg(not(unix))]
struct OsSignalSource;

#[cfg(not(unix))]
impl OsSignalSource {
    fn new() -> io::Result<Self> {
        Ok(Self)
    }
}

#[cfg(not(unix))]
impl SignalSource for OsSignalSource {
    async fn receive(&mut self) -> io::Result<ShutdownSignal> {
        tokio::signal::ctrl_c()
            .await
            .map(|()| ShutdownSignal::CtrlC)
    }
}

pub async fn serve(
    listener: TcpListener,
    app: Router,
    lifecycle: Lifecycle,
    config: &AppConfig,
) -> AppResult<()> {
    let signals = OsSignalSource::new().map_err(|source| AppError::ShutdownSignal { source })?;
    serve_with_signals(
        listener,
        app,
        lifecycle,
        ShutdownSettings::from(config),
        signals,
    )
    .await
}

async fn serve_with_signals<S>(
    listener: TcpListener,
    app: Router,
    lifecycle: Lifecycle,
    settings: ShutdownSettings,
    mut signals: S,
) -> AppResult<()>
where
    S: SignalSource,
{
    let (shutdown_sender, shutdown_receiver) = oneshot::channel::<()>();
    let server = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown_receiver.await;
        })
        .into_future();
    tokio::pin!(server);

    let signal = tokio::select! {
        result = &mut server => {
            result?;
            return Ok(());
        }
        signal = signals.receive() => signal
            .map_err(|source| AppError::ShutdownSignal { source })?,
    };

    let started_at = Instant::now();
    let deadline = started_at + settings.timeout;
    lifecycle.begin_shutdown();

    tracing::info!(
        event = "shutdown_signal_received",
        signal = %signal,
        "shutdown signal received"
    );
    tracing::info!(
        event = "shutdown_readiness_disabled",
        delay_ms = duration_ms(settings.readiness_delay),
        "readiness disabled before connection draining"
    );

    if !settings.readiness_delay.is_zero() {
        tokio::select! {
            result = &mut server => {
                result?;
                return Ok(());
            }
            signal = signals.receive() => {
                let signal = signal.map_err(|source| AppError::ShutdownSignal { source })?;
                return Err(forced_shutdown(signal, started_at));
            }
            () = sleep_until(started_at + settings.readiness_delay) => {}
        }
    }

    let _ = shutdown_sender.send(());
    tracing::info!(
        event = "shutdown_drain_started",
        remaining_ms = duration_ms(deadline.saturating_duration_since(Instant::now())),
        "connection draining started"
    );

    tokio::select! {
        result = &mut server => {
            result?;
            tracing::info!(
                event = "shutdown_completed",
                elapsed_ms = duration_ms(started_at.elapsed()),
                "graceful shutdown completed"
            );
            Ok(())
        }
        signal = signals.receive() => {
            let signal = signal.map_err(|source| AppError::ShutdownSignal { source })?;
            Err(forced_shutdown(signal, started_at))
        }
        () = sleep_until(deadline) => {
            tracing::error!(
                event = "shutdown_forced",
                reason = "timeout",
                elapsed_ms = duration_ms(started_at.elapsed()),
                timeout_ms = settings.timeout_ms,
                "graceful shutdown deadline exceeded"
            );
            Err(AppError::ShutdownTimeout {
                timeout_ms: settings.timeout_ms,
            })
        }
    }
}

fn forced_shutdown(signal: ShutdownSignal, started_at: Instant) -> AppError {
    tracing::error!(
        event = "shutdown_forced",
        reason = "second_signal",
        signal = %signal,
        elapsed_ms = duration_ms(started_at.elapsed()),
        "graceful shutdown forced by a second signal"
    );
    AppError::ShutdownForced {
        signal: signal.as_str(),
    }
}

fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use axum::{extract::State, routing::get};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        sync::{Notify, mpsc},
        time::{sleep, timeout},
    };

    use super::*;

    #[derive(Clone)]
    struct SlowRequestState {
        started: Arc<Notify>,
        release: Arc<Notify>,
    }

    struct TestSignalSource {
        receiver: mpsc::UnboundedReceiver<ShutdownSignal>,
    }

    impl SignalSource for TestSignalSource {
        async fn receive(&mut self) -> io::Result<ShutdownSignal> {
            self.receiver.recv().await.ok_or_else(|| {
                io::Error::new(io::ErrorKind::BrokenPipe, "test signal channel closed")
            })
        }
    }

    async fn slow_request(State(state): State<SlowRequestState>) -> &'static str {
        state.started.notify_one();
        state.release.notified().await;
        "slow request completed"
    }

    async fn fast_request() -> &'static str {
        "fast request completed"
    }

    fn test_app(state: SlowRequestState) -> Router {
        Router::new()
            .route("/slow", get(slow_request))
            .route("/fast", get(fast_request))
            .with_state(state)
    }

    fn test_settings(readiness_delay_ms: u64, timeout_ms: u64) -> ShutdownSettings {
        ShutdownSettings {
            readiness_delay: Duration::from_millis(readiness_delay_ms),
            timeout: Duration::from_millis(timeout_ms),
            timeout_ms,
        }
    }

    async fn request(address: SocketAddr, path: &'static str) -> String {
        let mut stream = tokio::net::TcpStream::connect(address)
            .await
            .expect("test client should connect");
        stream
            .write_all(
                format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .expect("test request should be written");

        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .await
            .expect("test response should be read");
        String::from_utf8(response).expect("test response should be utf-8")
    }

    async fn wait_until_not_ready(lifecycle: &Lifecycle) {
        timeout(Duration::from_secs(1), async {
            while lifecycle.is_ready() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("shutdown should disable readiness");
    }

    fn assert_ok_response(response: &str, expected_body: &str) {
        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "unexpected response: {response}"
        );
        assert!(
            response.ends_with(expected_body),
            "unexpected response: {response}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn in_flight_request_completes_during_graceful_shutdown() {
        let request_state = SlowRequestState {
            started: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
        };
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener should bind");
        let address = listener
            .local_addr()
            .expect("test listener should have an address");
        let lifecycle = Lifecycle::new();
        let (signal_sender, signal_receiver) = mpsc::unbounded_channel();
        let server = tokio::spawn(serve_with_signals(
            listener,
            test_app(request_state.clone()),
            lifecycle.clone(),
            test_settings(100, 1_000),
            TestSignalSource {
                receiver: signal_receiver,
            },
        ));

        let slow_client = tokio::spawn(request(address, "/slow"));
        timeout(Duration::from_secs(1), request_state.started.notified())
            .await
            .expect("slow request should start");
        signal_sender
            .send(ShutdownSignal::Sigterm)
            .expect("test signal should be delivered");
        wait_until_not_ready(&lifecycle).await;

        let pre_drain_response = request(address, "/fast").await;
        assert_ok_response(&pre_drain_response, "fast request completed");

        sleep(Duration::from_millis(150)).await;
        request_state.release.notify_waiters();

        let slow_response = timeout(Duration::from_secs(1), slow_client)
            .await
            .expect("slow client should finish")
            .expect("slow client task should succeed");
        assert_ok_response(&slow_response, "slow request completed");
        timeout(Duration::from_secs(1), server)
            .await
            .expect("server should finish")
            .expect("server task should succeed")
            .expect("graceful shutdown should succeed");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shutdown_timeout_returns_an_error() {
        let request_state = SlowRequestState {
            started: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
        };
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener should bind");
        let address = listener
            .local_addr()
            .expect("test listener should have an address");
        let lifecycle = Lifecycle::new();
        let (signal_sender, signal_receiver) = mpsc::unbounded_channel();
        let server = tokio::spawn(serve_with_signals(
            listener,
            test_app(request_state.clone()),
            lifecycle,
            test_settings(0, 100),
            TestSignalSource {
                receiver: signal_receiver,
            },
        ));

        let slow_client = tokio::spawn(request(address, "/slow"));
        timeout(Duration::from_secs(1), request_state.started.notified())
            .await
            .expect("slow request should start");
        signal_sender
            .send(ShutdownSignal::Sigterm)
            .expect("test signal should be delivered");

        let result = timeout(Duration::from_secs(1), server)
            .await
            .expect("server should enforce its deadline")
            .expect("server task should succeed");
        assert!(matches!(
            result,
            Err(AppError::ShutdownTimeout { timeout_ms: 100 })
        ));

        request_state.release.notify_waiters();
        let _ = timeout(Duration::from_secs(1), slow_client).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn second_signal_forces_shutdown() {
        let request_state = SlowRequestState {
            started: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
        };
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener should bind");
        let address = listener
            .local_addr()
            .expect("test listener should have an address");
        let lifecycle = Lifecycle::new();
        let (signal_sender, signal_receiver) = mpsc::unbounded_channel();
        let server = tokio::spawn(serve_with_signals(
            listener,
            test_app(request_state.clone()),
            lifecycle.clone(),
            test_settings(500, 1_000),
            TestSignalSource {
                receiver: signal_receiver,
            },
        ));

        let slow_client = tokio::spawn(request(address, "/slow"));
        timeout(Duration::from_secs(1), request_state.started.notified())
            .await
            .expect("slow request should start");
        signal_sender
            .send(ShutdownSignal::Sigterm)
            .expect("first test signal should be delivered");
        wait_until_not_ready(&lifecycle).await;
        signal_sender
            .send(ShutdownSignal::CtrlC)
            .expect("second test signal should be delivered");

        let result = timeout(Duration::from_secs(1), server)
            .await
            .expect("second signal should stop waiting")
            .expect("server task should succeed");
        assert!(matches!(
            result,
            Err(AppError::ShutdownForced { signal: "SIGINT" })
        ));

        request_state.release.notify_waiters();
        let _ = timeout(Duration::from_secs(1), slow_client).await;
    }
}
