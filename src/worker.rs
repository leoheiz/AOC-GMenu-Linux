//! Worker DDC serializado + debounce de sliders (~150 ms).

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use crate::backend::{Backend, BackendError, DetectedMonitor};
use crate::ddc::INTER_FEATURE_DELAY_MS;
use crate::features::{find_feature, FEATURES};
use crate::logutil;

pub const SLIDER_DEBOUNCE_MS: u64 = 150;

#[derive(Debug, Clone)]
pub enum WorkerCmd {
    /// Lê todas as features confirmadas.
    Reload,
    Get { feature_id: String },
    /// Escrita imediata (dropdown / toggle).
    SetNow { feature_id: String, value: u16 },
    /// Escrita com debounce (slider): só o último valor de cada feature é enviado.
    SetDebounced { feature_id: String, value: u16 },
    /// Flush forçado do debounce (ex.: ao fechar).
    FlushDebounce,
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum WorkerEvent {
    Status {
        bus: Option<u32>,
        model: Option<String>,
        connector: Option<String>,
        error: Option<String>,
    },
    Values(HashMap<String, FeatureValue>),
    Written {
        feature_id: String,
        value: u16,
    },
    Error(String),
}

#[derive(Debug, Clone)]
pub struct FeatureValue {
    pub current: u16,
    pub maximum: u16,
    pub error: Option<String>,
}

/// Estado puro de debounce (testável sem threads).
#[derive(Debug, Default, Clone)]
pub struct DebounceState {
    pending: HashMap<String, (u16, Instant)>,
    delay: Duration,
}

impl DebounceState {
    pub fn new(delay: Duration) -> Self {
        Self {
            pending: HashMap::new(),
            delay,
        }
    }

    pub fn with_default_delay() -> Self {
        Self::new(Duration::from_millis(SLIDER_DEBOUNCE_MS))
    }

    /// Registra mudança; retorna o valor anterior pendente (se houver) para a mesma feature.
    pub fn push(&mut self, feature_id: String, value: u16, now: Instant) -> Option<u16> {
        self.pending
            .insert(feature_id, (value, now))
            .map(|(v, _)| v)
    }

    /// Features cujo debounce expirou em `now`.
    pub fn ready(&mut self, now: Instant) -> Vec<(String, u16)> {
        let delay = self.delay;
        let mut ready = Vec::new();
        self.pending.retain(|id, (val, t)| {
            if now.duration_since(*t) >= delay {
                ready.push((id.clone(), *val));
                false
            } else {
                true
            }
        });
        ready
    }

    /// Drena tudo imediatamente.
    pub fn flush(&mut self) -> Vec<(String, u16)> {
        self.pending
            .drain()
            .map(|(id, (v, _))|
                (id, v))
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn pending_value(&self, feature_id: &str) -> Option<u16> {
        self.pending.get(feature_id).map(|(v, _)| *v)
    }

    pub fn clear(&mut self, feature_id: &str) {
        self.pending.remove(feature_id);
    }
}

pub struct WorkerHandle {
    pub cmd_tx: Sender<WorkerCmd>,
    pub event_rx: Receiver<WorkerEvent>,
}

impl WorkerHandle {
    /// Spawna worker com backend real (i2c). `monitor` já detectado.
    pub fn spawn_i2c(monitor: DetectedMonitor) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let bus = monitor.bus;
        let model = monitor.edid.model_name.clone();
        let connector = monitor.connector.clone();
        thread::spawn(move || {
            let mut backend = crate::backend::I2cBackend::new(bus);
            let _ = event_tx.send(WorkerEvent::Status {
                bus: Some(bus),
                model,
                connector: Some(connector),
                error: None,
            });
            run_worker_loop(&mut backend, cmd_rx, event_tx);
        });
        Self { cmd_tx, event_rx }
    }

    /// Worker com backend mock (testes / dry-run).
    pub fn spawn_with_backend<B>(mut backend: B) -> Self
    where
        B: Backend + 'static,
    {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        thread::spawn(move || {
            run_worker_loop(&mut backend, cmd_rx, event_tx);
        });
        Self { cmd_tx, event_rx }
    }
}

fn run_worker_loop<B: Backend>(
    backend: &mut B,
    cmd_rx: Receiver<WorkerCmd>,
    event_tx: Sender<WorkerEvent>,
) {
    let mut debounce = DebounceState::with_default_delay();
    loop {
        // Espera comando ou timeout curto para checar debounce.
        let cmd = match cmd_rx.recv_timeout(Duration::from_millis(25)) {
            Ok(c) => Some(c),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };

        if let Some(cmd) = cmd {
            match cmd {
                WorkerCmd::Shutdown => {
                    for (id, val) in debounce.flush() {
                        let _ = write_feature(backend, &event_tx, &id, val);
                    }
                    break;
                }
                WorkerCmd::FlushDebounce => {
                    for (id, val) in debounce.flush() {
                        let _ = write_feature(backend, &event_tx, &id, val);
                    }
                }
                WorkerCmd::Reload => {
                    let values = read_confirmed(backend);
                    let _ = event_tx.send(WorkerEvent::Values(values));
                }
                WorkerCmd::Get { feature_id } => {
                    match read_one(backend, &feature_id) {
                        Ok(v) => {
                            let mut m = HashMap::new();
                            m.insert(feature_id, v);
                            let _ = event_tx.send(WorkerEvent::Values(m));
                        }
                        Err(e) => {
                            let _ = event_tx.send(WorkerEvent::Error(e));
                        }
                    }
                }
                WorkerCmd::SetNow { feature_id, value } => {
                    // Cancela debounce pendente da mesma feature.
                    debounce.clear(&feature_id);
                    let _ = write_feature(backend, &event_tx, &feature_id, value);
                }
                WorkerCmd::SetDebounced { feature_id, value } => {
                    debounce.push(feature_id, value, Instant::now());
                }
            }
        }

        for (id, val) in debounce.ready(Instant::now()) {
            let _ = write_feature(backend, &event_tx, &id, val);
        }

        // Drena rajadas sem bloquear.
        loop {
            match cmd_rx.try_recv() {
                Ok(WorkerCmd::SetDebounced { feature_id, value }) => {
                    debounce.push(feature_id, value, Instant::now());
                }
                Ok(other) => {
                    // Reprocessa no próximo ciclo: reenvia via canal local simples —
                    // trataremos recursivamente empurrando de volta... melhor: handle inline.
                    match other {
                        WorkerCmd::Shutdown => {
                            for (id, val) in debounce.flush() {
                                let _ = write_feature(backend, &event_tx, &id, val);
                            }
                            return;
                        }
                        WorkerCmd::FlushDebounce => {
                            for (id, val) in debounce.flush() {
                                let _ = write_feature(backend, &event_tx, &id, val);
                            }
                        }
                        WorkerCmd::Reload => {
                            let values = read_confirmed(backend);
                            let _ = event_tx.send(WorkerEvent::Values(values));
                        }
                        WorkerCmd::Get { feature_id } => match read_one(backend, &feature_id) {
                            Ok(v) => {
                                let mut m = HashMap::new();
                                m.insert(feature_id, v);
                                let _ = event_tx.send(WorkerEvent::Values(m));
                            }
                            Err(e) => {
                                let _ = event_tx.send(WorkerEvent::Error(e));
                            }
                        },
                        WorkerCmd::SetNow { feature_id, value } => {
                            debounce.clear(&feature_id);
                            let _ = write_feature(backend, &event_tx, &feature_id, value);
                        }
                        WorkerCmd::SetDebounced { .. } => unreachable!(),
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
    }
}

fn read_confirmed<B: Backend>(backend: &mut B) -> HashMap<String, FeatureValue> {
    let mut out = HashMap::new();
    for f in FEATURES.iter().filter(|f| f.confirmed) {
        match backend.get(f.access) {
            Ok(r) => {
                out.insert(
                    f.id.to_string(),
                    FeatureValue {
                        current: r.current,
                        maximum: r.maximum,
                        error: None,
                    },
                );
            }
            Err(BackendError::Unsupported) => {
                out.insert(
                    f.id.to_string(),
                    FeatureValue {
                        current: 0,
                        maximum: 0,
                        error: Some("não suportado".into()),
                    },
                );
            }
            Err(e) => {
                logutil::tech(format!("reload {}: {e}", f.id));
                out.insert(
                    f.id.to_string(),
                    FeatureValue {
                        current: 0,
                        maximum: 0,
                        error: Some(e.user_message()),
                    },
                );
            }
        }
        thread::sleep(Duration::from_millis(INTER_FEATURE_DELAY_MS));
    }
    out
}

fn read_one<B: Backend>(backend: &mut B, feature_id: &str) -> Result<FeatureValue, String> {
    let f = find_feature(feature_id).ok_or_else(|| format!("feature desconhecida: {feature_id}"))?;
    match backend.get(f.access) {
        Ok(r) => Ok(FeatureValue {
            current: r.current,
            maximum: r.maximum,
            error: None,
        }),
        Err(BackendError::Unsupported) => Ok(FeatureValue {
            current: 0,
            maximum: 0,
            error: Some("não suportado".into()),
        }),
        Err(e) => {
            logutil::tech(format!("get {feature_id}: {e}"));
            Err(e.user_message())
        }
    }
}

fn write_feature<B: Backend>(
    backend: &mut B,
    event_tx: &Sender<WorkerEvent>,
    feature_id: &str,
    value: u16,
) -> Result<(), ()> {
    let Some(f) = find_feature(feature_id) else {
        let _ = event_tx.send(WorkerEvent::Error(format!(
            "feature desconhecida: {feature_id}"
        )));
        return Err(());
    };
    match backend.set(f.access, value) {
        Ok(()) => {
            let _ = event_tx.send(WorkerEvent::Written {
                feature_id: feature_id.to_string(),
                value,
            });
            Ok(())
        }
        Err(e) => {
            logutil::tech(format!("set {feature_id}: {e}"));
            let _ = event_tx.send(WorkerEvent::Error(format!(
                "{feature_id}: {}",
                e.user_message()
            )));
            Err(())
        }
    }
}

/// Fila de comandos pura (sem I2C): útil para testes de ordenação.
#[derive(Debug, Default)]
pub struct CmdQueue {
    pub items: Vec<WorkerCmd>,
}

impl CmdQueue {
    pub fn push(&mut self, cmd: WorkerCmd) {
        // Coalesce SetDebounced consecutivos da mesma feature.
        if let WorkerCmd::SetDebounced {
            feature_id: ref id,
            value,
        } = cmd
        {
            if let Some(WorkerCmd::SetDebounced {
                feature_id: last_id,
                value: last_val,
            }) = self.items.last_mut()
            {
                if last_id == id {
                    *last_val = value;
                    return;
                }
            }
        }
        self.items.push(cmd);
    }

    pub fn pop_front(&mut self) -> Option<WorkerCmd> {
        if self.items.is_empty() {
            None
        } else {
            Some(self.items.remove(0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::MockBackend;
    use crate::features::Access;

    #[test]
    fn debounce_keeps_last_value() {
        let mut d = DebounceState::new(Duration::from_millis(150));
        let t0 = Instant::now();
        d.push("brightness".into(), 10, t0);
        d.push("brightness".into(), 20, t0 + Duration::from_millis(10));
        d.push("brightness".into(), 55, t0 + Duration::from_millis(20));
        assert_eq!(d.pending_value("brightness"), Some(55));
        assert!(d.ready(t0 + Duration::from_millis(100)).is_empty());
        let ready = d.ready(t0 + Duration::from_millis(200));
        assert_eq!(ready, vec![("brightness".into(), 55)]);
        assert!(d.is_empty());
    }

    #[test]
    fn debounce_independent_features() {
        let mut d = DebounceState::new(Duration::from_millis(50));
        let t0 = Instant::now();
        d.push("brightness".into(), 1, t0);
        d.push("contrast".into(), 2, t0);
        let ready = d.ready(t0 + Duration::from_millis(60));
        assert_eq!(ready.len(), 2);
    }

    #[test]
    fn cmd_queue_coalesces_debounced() {
        let mut q = CmdQueue::default();
        q.push(WorkerCmd::SetDebounced {
            feature_id: "brightness".into(),
            value: 1,
        });
        q.push(WorkerCmd::SetDebounced {
            feature_id: "brightness".into(),
            value: 2,
        });
        q.push(WorkerCmd::SetDebounced {
            feature_id: "brightness".into(),
            value: 99,
        });
        assert_eq!(q.items.len(), 1);
        match &q.items[0] {
            WorkerCmd::SetDebounced { value, .. } => assert_eq!(*value, 99),
            _ => panic!(),
        }
    }

    #[test]
    fn worker_queue_with_mock_backend() {
        let mut mock = MockBackend::new();
        mock.seed(Access::std(0x10), 80, 100);
        mock.seed(Access::std(0x12), 60, 100);
        let handle = WorkerHandle::spawn_with_backend(mock);
        handle
            .cmd_tx
            .send(WorkerCmd::SetNow {
                feature_id: "brightness".into(),
                value: 42,
            })
            .unwrap();
        handle
            .cmd_tx
            .send(WorkerCmd::SetNow {
                feature_id: "contrast".into(),
                value: 33,
            })
            .unwrap();
        handle.cmd_tx.send(WorkerCmd::Shutdown).unwrap();

        let mut written = Vec::new();
        while let Ok(ev) = handle.event_rx.recv_timeout(Duration::from_secs(2)) {
            if let WorkerEvent::Written { feature_id, value } = ev {
                written.push((feature_id, value));
            }
        }
        assert!(written.contains(&("brightness".into(), 42)));
        assert!(written.contains(&("contrast".into(), 33)));
    }

    #[test]
    fn debounce_flush_order() {
        let mut d = DebounceState::with_default_delay();
        let t0 = Instant::now();
        d.push("a".into(), 1, t0);
        d.push("b".into(), 2, t0);
        let flushed = d.flush();
        assert_eq!(flushed.len(), 2);
        assert!(d.is_empty());
    }

}
